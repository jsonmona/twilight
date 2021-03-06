#ifndef TWILIGHT_SERVER_PLATFORM_SOFTWARE_ENCODEROPENH264_H
#define TWILIGHT_SERVER_PLATFORM_SOFTWARE_ENCODEROPENH264_H

#include "common/ByteBuffer.h"
#include "common/DesktopFrame.h"
#include "common/StatisticMixer.h"
#include "common/ffmpeg-headers.h"
#include "common/log.h"

#include "common/platform/software/OpenH264Loader.h"
#include "common/platform/software/TextureSoftware.h"

#include "server/LocalClock.h"

#include <deque>

class EncoderOpenH264 {
public:
    explicit EncoderOpenH264(LocalClock& clock);
    ~EncoderOpenH264();

    template <typename Fn>
    void setDataAvailableCallback(Fn fn) {
        onDataAvailable = std::move(fn);
    }

    void start();
    void stop();

    void setResolution(int width, int height);

    void pushData(DesktopFrame<TextureSoftware>&& newData);

private:
    void run_();

    static NamedLogger log;

    LocalClock& clock;
    std::function<void(DesktopFrame<ByteBuffer>&&)> onDataAvailable;

    std::shared_ptr<OpenH264Loader> loader;
    int width, height;

    bool nextFrameAvailable;
    std::atomic<bool> flagRun;

    std::thread runThread;
    std::mutex dataLock;
    std::condition_variable dataCV;

    DesktopFrame<TextureSoftware> nextFrame;
};

#endif
