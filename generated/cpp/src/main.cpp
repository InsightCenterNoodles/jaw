#include "example.hpp"
#include "codec.hpp"

#include <cstdio>
#include <cstddef>
#include <string>
#include <vector>
#include <fstream>

static void demand(bool condition, size_t at, const char* label) {
    if (condition) return;
    std::printf("Error line %zu: %s\n", at, label);
    exit(EXIT_FAILURE);
}

#define DEMAND(C) demand(C, __LINE__, #C)

std::vector<uint8_t> operator""_bytes(const char* str, std::size_t size) {
    return std::vector<uint8_t>(reinterpret_cast<const uint8_t*>(str),
                                reinterpret_cast<const uint8_t*>(str) + size);
}

static std::vector<uint8_t> to_bytes(const std::string& s) {
    return std::vector<uint8_t>(reinterpret_cast<const uint8_t*>(s.data()),
                                reinterpret_cast<const uint8_t*>(s.data()) + s.size());
}

bool operator==(example::ArrayRef<uint8_t> a, const char* str) {
    // slow but who cares, this is for testing
    auto local = std::string(str);

    auto bytes = to_bytes(local);

    std::vector<std::uint8_t> a_local;
    a.copy_to_vector(a_local);

    return std::equal(
        a_local.begin(), a_local.end(), bytes.begin(), bytes.end());
}

template <class T>
bool operator==(example::ArrayRef<T> a, std::vector<T> const& b) {
    std::vector<T> local;
    a.copy_to_vector(local);
    return std::equal(local.begin(), local.end(), b.begin(), b.end());
}

static void usage() {
    std::printf("Usage: driver [--dump PATH] [--read PATH]\n");
}

static std::vector<std::byte> read_all(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    if (!f) { std::printf("Error: failed to open %s\n", path.c_str()); return {}; }
    f.seekg(0, std::ios::end);
    std::streamsize sz = f.tellg();
    f.seekg(0, std::ios::beg);
    std::vector<std::byte> buf;
    buf.resize(static_cast<size_t>(sz));
    if (sz > 0) {
        f.read(reinterpret_cast<char*>(buf.data()), sz);
    }
    return buf;
}

static bool write_all(const std::string& path, std::span<const std::byte> data) {
    std::ofstream f(path, std::ios::binary);
    if (!f) { std::printf("Error: failed to open %s for write\n", path.c_str()); return false; }
    if (!data.empty()) {
        f.write(reinterpret_cast<const char*>(data.data()), static_cast<std::streamsize>(data.size()));
    }
    return (bool)f;
}

static void validate(std::span<std::byte> data) {
    using namespace example::readers;

    auto reader = VecReader { .src = data };

    {
        RootReader root;
        DEMAND(read(reader, root));

        DEMAND(root.name == "pod-one");
        auto ref = std::get<MyPODReader>(root.var);
        DEMAND(ref.a_thing == 10);
        DEMAND(ref.b_thing == 20);
    }

    {
        RootReader root;
        DEMAND(read(reader, root));

        DEMAND(root.name == "");
        auto ref = std::get<MyOtherPODReader>(root.var);

        DEMAND(ref.first.a_thing == 1);
        DEMAND(ref.first.b_thing == 0x1122334455667788ULL);

        auto t = std::array<std::uint8_t, 4> { 1, 2, 3, 4 };

        DEMAND(ref.second == t);
    }

    {
        RootReader root;
        DEMAND(read(reader, root));

        DEMAND(root.name == "void");
        auto ref = std::get<std::monostate>(root.var);
    }

    {
        RootReader root;
        DEMAND(read(reader, root));

        DEMAND(root.name == "floats");
        auto ref = std::get<SmallSeqReader>(root.var);

        auto data = std::vector<float> { 1.0f, 2.5f, -3.25f, 0.0f };

        DEMAND(ref.list == data);
    }

    {
        RootReader root;
        DEMAND(read(reader, root));

        DEMAND(root.name == "pod-two");
        auto ref = std::get<MyPODReader>(root.var);

        DEMAND(ref.a_thing == 255);
        DEMAND(ref.b_thing == 0xCAFEBABECAFED00DULL);
    }

    {
        RootReader root;
        DEMAND(read(reader, root));

        DEMAND(root.name == "seq-long");

        auto data = std::vector<float> {};

        for (int i = 0; i < 10; ++i)
            data.push_back(static_cast<float>(i) * 0.5f);

        auto ref = std::get<SmallSeqReader>(root.var);

        DEMAND(ref.list == data);
    }

    std::printf("Validation complete\n");
}

int main(int argc, char** argv) {
    std::string dump_path;
    std::string read_path;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a == "--dump" && i + 1 < argc) { dump_path = argv[++i]; }
        else if (a == "--read" && i + 1 < argc) { read_path = argv[++i]; }
        else { usage(); }
    }

    using namespace example::writers;


    // If reading from a dump, verify it matches the expected dataset
    if (!read_path.empty()) {
        auto content = read_all(read_path);

        validate(content);

    } else {

        // Build several Root messages with different MyVariant alternatives

        std::vector<std::byte> content;

        auto writer = VecWriter { .dest = content };

        // 1) Variant = MyPOD
        {
            auto pod_one = "pod-one"_bytes;

            auto mypod = MyPODWriter { .a_thing = 10, .b_thing = 20 };

            RootWriter r {
                .name = pod_one,
                .var  = &mypod,
            };

            DEMAND(write(writer, r));
        }

        // 2) Variant = MyOtherPOD
        {
            auto empty = ""_bytes;

            auto other = MyOtherPODWriter {
                .first  = MyPODWriter { .a_thing = 1,
                                        .b_thing = 0x1122334455667788ULL },
                .second = { 1, 2, 3, 4 }
            };

            RootWriter r {
                .name = empty,
                .var  = &other,
            };

            DEMAND(write(writer, r));
        }

        // 3) Variant = void
        {
            auto name = "void"_bytes;

            RootWriter r {
                .name = name,
                .var  = std::monostate(),
            };

            DEMAND(write(writer, r));
        }

        // 4) Variant = SmallSeq with floats
        {

            auto name = "floats"_bytes;

            auto data = std::vector<float> { 1.0f, 2.5f, -3.25f, 0.0f };

            SmallSeqWriter seq { .list = data };

            RootWriter r {
                .name = name,
                .var  = &seq,
            };


            DEMAND(write(writer, r));
        }

        // 5) Another MyPOD with larger values
        {

            auto name = "pod-two"_bytes;

            auto my_pod = MyPODWriter { .a_thing = 255,
                                        .b_thing = 0xCAFEBABECAFED00DULL };


            RootWriter r {
                .name = name,
                .var  = &my_pod,
            };

            DEMAND(write(writer, r));
        }

        // 6) Another SmallSeq, longer
        {
            auto name = "seq-long"_bytes;

            auto data = std::vector<float> {};

            for (int i = 0; i < 10; ++i)
                data.push_back(static_cast<float>(i) * 0.5f);


            SmallSeqWriter seq { .list = data };

            RootWriter r {
                .name = name,
                .var  = &seq,
            };

            DEMAND(write(writer, r));
        }

        validate(content);


        if (!dump_path.empty()) {
            if (write_all(dump_path, std::span<const std::byte>(content.data(), content.size()))) {
                std::printf("Wrote %zu bytes to %s\n", content.size(), dump_path.c_str());
            }
        }
    }
    return 0;
}
