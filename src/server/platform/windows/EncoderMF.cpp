#include "EncoderMF.h"

#include <chrono>
#include <deque>

TWILIGHT_DEFINE_LOGGER(EncoderMF);

static std::string intoUTF8(std::wstring_view wideStr) {
    static_assert(sizeof(wchar_t) == sizeof(WCHAR), "Expects wchar_t == WCHAR (from winnt)");

    std::string ret;
    ret.resize(wideStr.size() * 4);
    int usedSize = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wideStr.data(), wideStr.size(), ret.data(),
                                       ret.size(), nullptr, nullptr);

    if (usedSize >= 0) {
        ret.resize(usedSize);
        return ret;
    } else if (usedSize == ERROR_INSUFFICIENT_BUFFER) {
        int targetSize = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wideStr.data(), wideStr.size(), nullptr, -1,
                                             nullptr, nullptr);
        ret.resize(targetSize);
        usedSize = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wideStr.data(), wideStr.size(), ret.data(),
                                       ret.size(), nullptr, nullptr);
        ret.resize(usedSize);
        return ret;
    } else {
        // FIXME: Add a proper error logging mechanism
        ret.resize(0);
        ret.append("<Failed to convert wide string into UTF-8>");
        return ret;
    }
}

static MFTransform getVideoEncoder(const MFDxgiDeviceManager& deviceManager, const NamedLogger& log) {
    HRESULT hr;
    MFTransform transform;
    IMFActivate** mftActivate;
    UINT32 arraySize;

    MFT_REGISTER_TYPE_INFO outputType = {};
    outputType.guidMajorType = MFMediaType_Video;
    outputType.guidSubtype = MFVideoFormat_H264;

    hr = MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER, nullptr,
                   &outputType, &mftActivate, &arraySize);

    if (FAILED(hr)) {
        mftActivate = nullptr;
        arraySize = 0;
    }

    bool foundTransform = false;

    for (unsigned i = 0; i < arraySize; i++) {
        if (!foundTransform) {
            transform.release();
            hr = mftActivate[i]->ActivateObject(transform.guid(), (void**)transform.data());
            if (FAILED(hr))
                continue;

            MFAttributes attr;
            hr = transform->GetAttributes(attr.data());
            if (FAILED(hr))
                continue;

            UINT32 flagD3DAware = 0;
            attr->GetUINT32(MF_SA_D3D11_AWARE, &flagD3DAware);
            if (flagD3DAware == 0)
                continue;

            UINT32 flagAsync = 0;
            attr->GetUINT32(MF_TRANSFORM_ASYNC, &flagAsync);
            if (flagAsync == 0)
                continue;

            hr = attr->SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK, 1);
            if (FAILED(hr))
                continue;

            hr = transform->ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, (ULONG_PTR)deviceManager.ptr());
            if (FAILED(hr))
                continue;

            WCHAR* friendlyName = nullptr;
            UINT32 friendlyNameLen = 0;
            mftActivate[i]->GetAllocatedString(MFT_FRIENDLY_NAME_Attribute, &friendlyName, &friendlyNameLen);
            if (friendlyName != nullptr) {
                std::wstring_view strView(friendlyName, friendlyNameLen);
                log.info("Selecting MFT codec: {}", intoUTF8(strView));
            }
            CoTaskMemFree(friendlyName);

            foundTransform = true;
        }
        mftActivate[i]->Release();
    }

    if (mftActivate)
        CoTaskMemFree(mftActivate);

    if (!foundTransform)
        transform.release();

    return transform;
}

EncoderMF::EncoderMF(LocalClock& clock)
    : clock(clock), width(-1), height(-1), waitingInput(false), initialized(false) {}

EncoderMF::~EncoderMF() {}

void EncoderMF::init(DxgiHelper dxgiHelper) {}

void EncoderMF::open(D3D11Device device, D3D11DeviceContext context) {
    HRESULT hr;

    mfDeviceManager.release();

    hr = MFCreateDXGIDeviceManager(&resetToken, mfDeviceManager.data());
    log.assert_quit(SUCCEEDED(hr), "Failed to create MF DXGI device manager");

    hr = mfDeviceManager->ResetDevice(device.ptr(), resetToken);
    log.assert_quit(SUCCEEDED(hr), "Failed to reset DXGI device for MF");
}

void EncoderMF::start() {
    frameCnt = 0;
    initialized = false;
    waitingInput = true;
}

void EncoderMF::stop() {
    initialized = false;

    encoder->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, inputStreamId);
    encoder->ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
}

void EncoderMF::init_() {
    HRESULT hr;

    encoder = getVideoEncoder(mfDeviceManager, log);
    log.assert_quit(encoder.isValid(), "Failed to create encoder");

    VARIANT value;
    ComWrapper<ICodecAPI> codec = encoder.castTo<ICodecAPI>();

    // TODO: Lines with check_quit commented out means my test machine did not accept those properties.
    //      Check what E_INVALIDARG means, and if we could remove those lines

    InitVariantFromUInt32(eAVEncVideoSourceScan_Progressive, &value);
    hr = codec->SetValue(&CODECAPI_AVEncVideoForceSourceScanType, &value);
    // log.assert_quit(SUCCEEDED(hr), "Failed to set video to progressive scan");

    InitVariantFromUInt32(eAVEncCommonRateControlMode_LowDelayVBR, &value);
    hr = codec->SetValue(&CODECAPI_AVEncCommonRateControlMode, &value);
    log.assert_quit(SUCCEEDED(hr), "Failed to set low delay vbr mode");

    InitVariantFromUInt32(8 * 1000 * 1000, &value);
    hr = codec->SetValue(&CODECAPI_AVEncCommonMeanBitRate, &value);
    log.assert_quit(SUCCEEDED(hr), "Failed to set bitrate to 8Mbps");

    InitVariantFromBoolean(true, &value);
    hr = codec->SetValue(&CODECAPI_AVEncCommonRealTime, &value);
    // log.assert_quit(SUCCEEDED(hr), "Failed to enable real time mode");

    InitVariantFromBoolean(true, &value);
    hr = codec->SetValue(&CODECAPI_AVEncCommonLowLatency, &value);
    // log.assert_quit(SUCCEEDED(hr), "Failed to enable low latency mode");

    InitVariantFromUInt32(eAVEncVideoOutputFrameRateConversion_Disable, &value);
    hr = codec->SetValue(&CODECAPI_AVEncVideoOutputFrameRateConversion, &value);
    // log.assert_quit(SUCCEEDED(hr), "Failed to disable frame rate conversion");

    DWORD inputStreamCnt, outputStreamCnt;
    hr = encoder->GetStreamCount(&inputStreamCnt, &outputStreamCnt);
    log.assert_quit(SUCCEEDED(hr), "Failed to get stream count");
    if (inputStreamCnt != 1 || outputStreamCnt != 1)
        log.error_quit("Invalid number of stream: input={} output={}", inputStreamCnt, outputStreamCnt);

    hr = encoder->GetStreamIDs(1, &inputStreamId, 1, &outputStreamId);
    if (hr == E_NOTIMPL) {
        inputStreamId = 0;
        outputStreamId = 0;
    } else
        log.assert_quit(SUCCEEDED(hr), "Failed to duplicate output");

    if (inputStreamCnt < 1 || outputStreamCnt < 1)
        log.error_quit("Adding stream manually is not implemented");

    MFMediaType mediaType;
    GUID videoFormat;

    // FIXME: Below will set stream types, but only in output->input order.
    //       While it works with NVIDIA, This might fail in other devices.

    hr = encoder->GetOutputAvailableType(outputStreamId, 0, mediaType.data());
    if (SUCCEEDED(hr)) {
        // hr = mediaType->SetUINT32(MF_MT_AVG_BITRATE, 15 * 1000 * 1000);  // 15Mbps
        // hr = mediaType->SetUINT32(CODECAPI_AVEncCommonRateControlMode, eAVEncCommonRateControlMode_CBR);
        hr = MFSetAttributeRatio(mediaType.ptr(), MF_MT_FRAME_RATE, 60000, 1001);  // FIXME: Assuming 60fps
        hr = MFSetAttributeSize(mediaType.ptr(), MF_MT_FRAME_SIZE, width, height);
        hr = mediaType->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlaceMode::MFVideoInterlace_Progressive);
        hr = mediaType->SetUINT32(MF_MT_MPEG2_PROFILE, eAVEncH264VProfile::eAVEncH264VProfile_Base);
        hr = mediaType->SetUINT32(MF_LOW_LATENCY, 1);
        // TODO: Is there any way to find out if encoder DOES support low latency mode?
        // TODO: Test if using main + low latency gives nicer output
        hr = encoder->SetOutputType(outputStreamId, mediaType.ptr(), 0);
        log.assert_quit(SUCCEEDED(hr), "Failed to set output type");
    }

    GUID acceptableCodecList[] = {MFVideoFormat_NV12};

    int chosenType = -1;
    for (DWORD i = 0; i < MAXDWORD; i++) {
        mediaType.release();
        hr = encoder->GetInputAvailableType(inputStreamId, i, mediaType.data());
        if (hr == MF_E_NO_MORE_TYPES)
            break;
        if (SUCCEEDED(hr)) {
            hr = mediaType->GetGUID(MF_MT_SUBTYPE, &videoFormat);
            log.assert_quit(SUCCEEDED(hr), "Failed to query input type");

            for (int j = 0; j < sizeof(acceptableCodecList) / sizeof(GUID); j++) {
                if (memcmp(&videoFormat, &acceptableCodecList[j], sizeof(GUID)) == 0) {
                    chosenType = j;
                    break;
                }
            }

            if (chosenType != -1) {
                hr = encoder->SetInputType(inputStreamId, mediaType.ptr(), 0);
                log.assert_quit(SUCCEEDED(hr), "Failed to set input type");
                break;
            }
        }
    }

    log.assert_quit(chosenType != -1, "No supported input type found");
}

void EncoderMF::poll() {
    HRESULT hr;

    if (waitingInput || !initialized)
        return;

    while (true) {
        MFMediaEvent ev;

        hr = eventGen->GetEvent(MF_EVENT_FLAG_NO_WAIT, ev.data());
        if (hr == MF_E_SHUTDOWN || hr == MF_E_NO_EVENTS_AVAILABLE)
            break;
        log.assert_quit(SUCCEEDED(hr), "Failed to get next event ({})", hr);

        MediaEventType evType;
        ev->GetType(&evType);

        if (evType == METransformDrainComplete) {
            continue;
        } else if (evType == METransformNeedInput) {
            waitingInput = true;
        } else if (evType == METransformHaveOutput) {
            auto toRemove = extraData.end();
            long long sampleTime;
            bool isIDR;
            DesktopFrame<long long> now;

            ByteBuffer encoded = popEncoderData_(&sampleTime, &isIDR);
            for (auto itr = extraData.begin(); itr != extraData.end(); ++itr) {
                if (itr->desktop == sampleTime) {
                    now = std::move(*itr);
                    toRemove = itr;
                    break;
                }
            }

            log.assert_quit(toRemove != extraData.end(), "Failed to find matching ExtraData (size={}, sampleTime={})",
                            extraData.size(), sampleTime);

            extraData.erase(toRemove);

            now.timeEncoded = clock.time();
            now.isIDR = isIDR;
            onDataAvailable(now.getOtherType(std::move(encoded)));
        } else {
            log.warn("Ignoring unknown MediaEventType {}", static_cast<DWORD>(evType));
        }
    }
}

bool EncoderMF::pushFrame(DesktopFrame<D3D11Texture2D>* cap) {
    if (!waitingInput)
        return false;

    waitingInput = false;

    // FIXME: Does not accept changing resolution after first call
    if (!initialized) {
        initialized = true;

        D3D11_TEXTURE2D_DESC desc = {};
        cap->desktop.ptr()->GetDesc(&desc);
        width = desc.Width;
        height = desc.Height;

        init_();
        encoder->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, inputStreamId);
        eventGen = encoder.castTo<IMFMediaEventGenerator>();
    }

    // FIXME: Assuming 60fps
    long long MFTime = 10000000;  // 100-ns to sec (used by MediaFoundation)
    long long frameNum = 60, frameDen = 1;
    long long sampleDur = MFTime * frameDen / frameNum;
    long long sampleTime = frameCnt * MFTime * frameDen / frameNum;

    extraData.push_back(cap->getOtherType<long long>(std::move(sampleTime)));

    pushEncoderTexture_(cap->desktop, sampleDur, sampleTime);
    frameCnt++;
    return true;
}

void EncoderMF::pushEncoderTexture_(const D3D11Texture2D& tex, long long sampleDur, long long sampleTime) {
    HRESULT hr;

    MFMediaBuffer mediaBuffer;
    hr = MFCreateDXGISurfaceBuffer(tex.guid(), tex.ptr(), 0, false, mediaBuffer.data());
    log.assert_quit(SUCCEEDED(hr), "Failed to create media buffer containing D3D11 texture");

    MFSample sample;
    hr = MFCreateSample(sample.data());
    log.assert_quit(SUCCEEDED(hr), "Failed to create a sample");

    sample->AddBuffer(mediaBuffer.ptr());
    sample->SetSampleDuration(sampleDur);
    sample->SetSampleTime(sampleTime);

    hr = encoder->ProcessInput(0, sample.ptr(), 0);
    if (hr == MF_E_NOTACCEPTING)
        return;
    log.assert_quit(SUCCEEDED(hr), "Failed to put input into encoder");
}

ByteBuffer EncoderMF::popEncoderData_(long long* sampleTime, bool* isIDR) {
    HRESULT hr;

    MFT_OUTPUT_STREAM_INFO outputStreamInfo;
    hr = encoder->GetOutputStreamInfo(0, &outputStreamInfo);
    log.assert_quit(SUCCEEDED(hr), "Failed to get output stream info");

    bool shouldAllocateOutput =
        !(outputStreamInfo.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES));
    int allocSize = outputStreamInfo.cbSize + outputStreamInfo.cbAlignment * 2;
    log.assert_quit(!shouldAllocateOutput, "Allocating output is not implemented yet");

    DWORD status;
    MFT_OUTPUT_DATA_BUFFER outputBuffer = {};
    outputBuffer.dwStreamID = outputStreamId;
    hr = encoder->ProcessOutput(0, 1, &outputBuffer, &status);
    log.assert_quit(SUCCEEDED(hr), "Failed to retrieve output from encoder");

    if (sampleTime)
        outputBuffer.pSample->GetSampleTime(sampleTime);

    if (isIDR)
        *isIDR = MFGetAttributeUINT32(outputBuffer.pSample, MFSampleExtension_CleanPoint, 0);

    DWORD bufferCount = 0;
    hr = outputBuffer.pSample->GetBufferCount(&bufferCount);
    log.assert_quit(SUCCEEDED(hr), "Failed to get buffer count");

    DWORD totalLen = 0;
    hr = outputBuffer.pSample->GetTotalLength(&totalLen);
    log.assert_quit(SUCCEEDED(hr), "Failed to query total length of sample");

    ByteBuffer data(totalLen);
    size_t idx = 0;

    for (int j = 0; j < bufferCount; j++) {
        MFMediaBuffer mediaBuffer;
        hr = outputBuffer.pSample->GetBufferByIndex(j, mediaBuffer.data());
        log.assert_quit(SUCCEEDED(hr), "Failed to get buffer");

        BYTE* ptr;
        DWORD len;
        hr = mediaBuffer->Lock(&ptr, nullptr, &len);
        log.assert_quit(SUCCEEDED(hr), "Failed to lock buffer");

        memcpy(data.data() + idx, ptr, len);
        idx += len;

        hr = mediaBuffer->Unlock();
        log.assert_quit(SUCCEEDED(hr), "Failed to unlock buffer");
    }

    outputBuffer.pSample->Release();

    if (outputBuffer.pEvents)
        outputBuffer.pEvents->Release();

    return data;
}
