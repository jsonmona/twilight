#ifndef TWILIGHT_COMMON_BYTEBUFFER_H
#define TWILIGHT_COMMON_BYTEBUFFER_H

#include "common/log.h"

#include <mbedtls/base64.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <string>
#include <type_traits>

class ByteBuffer {
public:
    template <class T>
    class View {
        static_assert(std::is_trivial<T>::value, "Only trivial types can be used!");
        ByteBuffer *parent;

    public:
        explicit View(ByteBuffer *_parent) : parent(_parent) {}

        size_t size() const { return parent->nowSize / sizeof(T); }

        T *data() const { return reinterpret_cast<T *>(parent->ptr); }
        T *begin() const { return reinterpret_cast<T *>(parent->ptr); }
        T *end() const { return reinterpret_cast<T *>(parent->ptr) + size(); }
    };

    ByteBuffer() : ptr(nullptr), size_(0), capacity_(0) {}
    explicit ByteBuffer(size_t initialSize) : ptr(nullptr), size_(0), capacity_(0) { resize(initialSize); }
    ByteBuffer(const ByteBuffer &copy) = delete;
    ByteBuffer(ByteBuffer &&move) : ptr(nullptr), size_(0), capacity_(0) { swap(*this, move); }

    ByteBuffer &operator=(const ByteBuffer &copy) = delete;
    ByteBuffer &operator=(ByteBuffer &&move) {
        swap(*this, move);
        return *this;
    }

    friend void swap(ByteBuffer &a, ByteBuffer &b) {
        using std::swap;

        swap(a.ptr, b.ptr);
        swap(a.size_, b.size_);
        swap(a.capacity_, b.capacity_);
    }

    ~ByteBuffer() { free(ptr); }

    ByteBuffer clone() const {
        ByteBuffer ret;
        if (size() != 0) {
            ret.resize(size());
            memcpy(ret.data(), data(), size());
        }
        return ret;
    }

    void reserve(size_t newCapacity) {
        if (capacity_ < newCapacity) {
            void *newPtr;
            if (ptr != nullptr)
                newPtr = realloc(ptr, newCapacity);
            else
                newPtr = malloc(newCapacity);

            if (newPtr == nullptr)
                NamedLogger("ByteBuffer").error_quit("Failed to allocate memory");
            ptr = reinterpret_cast<uint8_t *>(newPtr);
            capacity_ = newCapacity;
        }
    }

    void resize(size_t newSize) {
        if (newSize == 0) {
            free(ptr);
            ptr = nullptr;
            size_ = 0;
            capacity_ = 0;
        } else if (newSize <= capacity_) {
            size_ = newSize;
        } else {
            reserve(newSize);
            size_ = newSize;
        }
    }

    void shrinkToFit() {
        if (capacity_ == size_ || ptr == nullptr)
            return;

        void *newPtr = realloc(ptr, size_);
        if (newPtr == nullptr)
            NamedLogger("ByteBuffer").error_quit("Failed to allocate memory");

        ptr = reinterpret_cast<uint8_t *>(newPtr);
        capacity_ = size_;
    }

    std::string intoHexString() const {
        static constexpr char HEX_DIGITS[] = "0123456789ABCDEF";
        std::string ret;
        ret.reserve(size_ * 2);
        for (size_t i = 0; i < size_; i++) {
            uint8_t val = data()[i];
            ret.push_back(HEX_DIGITS[val / 16]);
            ret.push_back(HEX_DIGITS[val % 16]);
        }
        return ret;
    }

    std::string intoBase64String() const {
        size_t olen = 4 * ((size_ + 2) / 3);
        std::string ret;
        ret.reserve(olen + 1);
        ret.resize(olen);

        int status = mbedtls_base64_encode(reinterpret_cast<uint8_t *>(ret.data()), olen + 1, &olen, data(), size_);
        if (status != 0 || ret.size() != olen)
            abort();  // FIXME: Use of abort

        return ret;
    }

    uint8_t &operator[](size_t idx) { return data()[idx]; }

    uint8_t operator[](size_t idx) const { return data()[idx]; }

    // removes content near begin
    void shiftTowardBegin(size_t amount) {
        if (amount != 0)
            memmove(ptr, ptr + amount, size() - amount);
    }

    // removes content near end
    void shiftTowardEnd(size_t amount) {
        if (amount != 0)
            memmove(ptr + amount, ptr, size() - amount);
    }

    void write(size_t dstOffset, const void *src, size_t length) {
        size_t newSize = std::max(size_, dstOffset + length);
        reserve(newSize);
        memcpy(ptr + dstOffset, src, length);
        size_ = newSize;
    }

    void write(size_t dstOffset, const ByteBuffer &other) {
        size_t newSize = std::max(size_, dstOffset + other.size());
        reserve(newSize);
        memcpy(ptr + dstOffset, other.data(), other.size());
        size_ = newSize;
    }

    void append(const void *src, size_t length) {
        reserve(size_ + length);
        memcpy(ptr + size_, src, length);
        size_ += length;
    }

    void append(const ByteBuffer &other) {
        reserve(size_ + other.size());
        memcpy(ptr + size_, other.data(), other.size());
        size_ += other.size();
    }

    void append(uint8_t data) {
        reserve(size_ + 1);
        ptr[size_] = data;
        size_++;
    }

    size_t capacity() const { return capacity_; }
    size_t size() const { return size_; }

    uint8_t *data() { return ptr; }
    uint8_t *begin() { return ptr; }
    uint8_t *end() { return ptr + size(); }

    const uint8_t *data() const { return ptr; }
    const uint8_t *begin() const { return ptr; }
    const uint8_t *end() const { return ptr + size(); }

    char *data_char() { return reinterpret_cast<char *>(ptr); }
    char *begin_char() { return reinterpret_cast<char *>(ptr); }
    char *end_char() { return reinterpret_cast<char *>(ptr + size()); }

    const char *data_char() const { return reinterpret_cast<const char *>(ptr); }
    const char *begin_char() const { return reinterpret_cast<const char *>(ptr); }
    const char *end_char() const { return reinterpret_cast<const char *>(ptr + size()); }

    template <class T>
    View<T> view() {
        return View<T>(this);
    }

private:
    uint8_t *ptr;
    size_t capacity_;
    size_t size_;
};

#endif
