#pragma once

#include <algorithm>
#include <span>
#include <vector>

struct VecWriter {
    std::vector<std::byte>& dest;

    bool write_bytes(std::byte const* v, size_t count) {
        dest.insert(dest.end(), v, v + count);
        return true;
    }
};


struct VecReader {
    std::span<std::byte const> src;

    bool read_bytes(std::byte* p, size_t count) {
        if (count > src.size()) { return false; }

        std::copy_n(src.begin(), count, p);

        src = src.subspan(count);

        return true;
    }
    std::span<const std::byte> advance_bytes(size_t count) {
        if (count > src.size()) { return {}; }

        auto ret = src.first(count);

        src = src.subspan(count);

        return ret;
    }
};
