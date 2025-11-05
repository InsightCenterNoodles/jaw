#pragma once

#include <span>
#include <vector>

struct VecWriter {
    std::vector<std::byte>& dest;

    bool append(std::span<const std::byte> v) {
        dest.insert(dest.end(), v.begin(), v.end());
        return true;
    }

    template <class T>
    bool write_raw(T const& t) {
        static_assert(std::is_trivial_v<T>);

        return append(std::as_bytes(std::span(&t, 1)));
    }
};


struct VecReader {
    std::span<std::byte const> src;

    bool read_to(std::span<std::byte> v) {

        if (v.size() > src.size()) { return false; }

        std::copy_n(src.begin(), v.size(), v.begin());

        src = src.subspan(v.size());

        return true;
    }

    template <class T>
    bool read_raw(T& t) {
        static_assert(std::is_trivial_v<T>);

        return read_to(std::as_writable_bytes(std::span(&t, 1)));
    }
};
