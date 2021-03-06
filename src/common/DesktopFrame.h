#ifndef TWILIGHT_COMMON_DESKTOPFRAME_H
#define TWILIGHT_COMMON_DESKTOPFRAME_H

#include "common/ByteBuffer.h"

#include <chrono>
#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

// TODO: Move this to a more proper place
enum class ScaleType { AYUV, NV12 };

enum class CodecType { INVALID, H264_BASELINE, VP8 };

enum class CursorShapeFormat { RGBA, RGBA_XOR };

struct CursorPos;
struct CursorShape;

template <typename T>
struct DesktopFrame {
    T desktop;
    std::shared_ptr<CursorPos> cursorPos;
    std::shared_ptr<CursorShape> cursorShape;

    std::chrono::microseconds timeCaptured;
    std::chrono::microseconds timeEncoded;
    std::chrono::microseconds timeReceived;
    std::chrono::microseconds timeDecoded;
    std::chrono::microseconds timePresented;

    bool isIDR;

    template <typename U>
    DesktopFrame<U> getOtherType(U&& newDesktop) {
        DesktopFrame<U> ret;
        ret.desktop = std::move(newDesktop);
        ret.cursorPos = cursorPos;
        ret.cursorShape = cursorShape;

        ret.timeCaptured = timeCaptured;
        ret.timeEncoded = timeEncoded;
        ret.timeReceived = timeReceived;
        ret.timeDecoded = timeDecoded;
        ret.timePresented = timePresented;

        ret.isIDR = isIDR;
        return ret;
    }

    DesktopFrame()
        : desktop(),
          cursorPos(),
          cursorShape(),
          timeCaptured(-1),
          timeEncoded(-1),
          timeReceived(-1),
          timeDecoded(-1),
          timePresented(-1),
          isIDR(false) {}
    DesktopFrame(const DesktopFrame& copy) = delete;
    DesktopFrame(DesktopFrame&& move) = default;
    DesktopFrame& operator=(const DesktopFrame& copy) = delete;
    DesktopFrame& operator=(DesktopFrame&& move) = default;

    friend void swap(DesktopFrame<T>& a, DesktopFrame<T>& b) {
        using std::swap;

        swap(a.desktop, b.desktop);
        swap(a.cursorPos, b.cursorPos);
        swap(a.cursorShape, b.cursorShape);
        swap(a.timeCaptured, b.timeCaptured);
        swap(a.timeEncoded, b.timeEncoded);
        swap(a.timeReceived, b.timeReceived);
        swap(a.timeDecoded, b.timeDecoded);
        swap(a.timePresented, b.timePresented);
        swap(a.isIDR, b.isIDR);
    }
};

struct CursorPos {
    bool visible;
    int x, y;
};

struct CursorShape {
    int width, height;
    int hotspotX, hotspotY;
    CursorShapeFormat format;
    ByteBuffer image;
};

#endif
