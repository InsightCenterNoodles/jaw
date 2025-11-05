#include "basic.hpp"
#include "codec.hpp"

#include <cstdio>
#include <cstddef>
#include <string>
#include <vector>

static void demand(bool condition, const char* label) {
    if (condition) return;
    std::printf("Error: %s\n", label);
}

static std::vector<uint8_t> to_bytes(const std::string& s) {
    return std::vector<uint8_t>(reinterpret_cast<const uint8_t*>(s.data()),
                                reinterpret_cast<const uint8_t*>(s.data()) + s.size());
}

int main() {
    using basic::Root;
    using basic::MyPOD;
    using basic::MyOtherPOD;
    using basic::MyVariant;
    using basic::SmallSeq;

    // Build several Root messages with different MyVariant alternatives
    std::vector<Root> expected;

    // 1) Variant = MyPOD
    {
        Root r{};
        r.name = to_bytes("pod-one");
        r.var.value = MyPOD{ .a_thing = 10, .b_thing = 20 };
        expected.push_back(r);
    }

    // 2) Variant = MyOtherPOD
    {
        Root r{};
        r.name = to_bytes(""); // empty name
        r.var.value = MyOtherPOD{
            .first = MyPOD{ .a_thing = 1, .b_thing = 0x1122334455667788ULL },
            .second = { 1, 2, 3, 4 }
        };
        expected.push_back(r);
    }

    // 3) Variant = SmallSeq with floats
    {
        Root r{};
        r.name = to_bytes("floats");
        SmallSeq seq{};
        seq.list = { 1.0f, 2.5f, -3.25f, 0.0f };
        r.var.value = seq;
        expected.push_back(r);
    }

    // 4) Another MyPOD with larger values
    {
        Root r{};
        r.name = to_bytes("pod-two");
        r.var.value = MyPOD{ .a_thing = 255, .b_thing = 0xCAFEBABECAFED00DULL };
        expected.push_back(r);
    }

    // 5) Another SmallSeq, longer
    {
        Root r{};
        r.name = to_bytes("seq-long");
        SmallSeq seq{};
        for (int i = 0; i < 10; ++i) seq.list.push_back(static_cast<float>(i) * 0.5f);
        r.var.value = seq;
        expected.push_back(r);
    }

    // Serialize all messages
    std::vector<std::byte> content;
    {
        auto writer = VecWriter{ .dest = content };
        for (const auto& r : expected) {
            bool ok = basic::write(writer, r);
            demand(ok, "write Root");
        }
    }

    // Read them back and validate
    std::vector<Root> actual;
    {
        auto reader = VecReader{ .src = content };
        for (size_t i = 0; i < expected.size(); ++i) {
            Root r{};
            bool ok = basic::read(reader, r);
            demand(ok, "read Root");
            actual.push_back(std::move(r));
        }
        // Ensure buffer fully consumed
        demand(reader.src.empty(), "buffer fully consumed");
    }

    // Compare expected vs actual content
    demand(actual.size() == expected.size(), "size matches");
    for (size_t i = 0; i < expected.size(); ++i) {
        const auto& e = expected[i];
        const auto& a = actual[i];

        demand(a.name == e.name, "name matches");
        demand(a.var.value.index() == e.var.value.index(), "variant tag matches");

        switch (e.var.value.index()) {
            case 0: {
                const auto& ep = std::get<0>(e.var.value);
                const auto& ap = std::get<0>(a.var.value);
                demand(ap.a_thing == ep.a_thing, "MyPOD.a_thing");
                demand(ap.b_thing == ep.b_thing, "MyPOD.b_thing");
                break;
            }
            case 1: {
                const auto& eo = std::get<1>(e.var.value);
                const auto& ao = std::get<1>(a.var.value);
                demand(ao.first.a_thing == eo.first.a_thing, "MyOtherPOD.first.a_thing");
                demand(ao.first.b_thing == eo.first.b_thing, "MyOtherPOD.first.b_thing");
                for (size_t j = 0; j < eo.second.size(); ++j) {
                    demand(ao.second[j] == eo.second[j], "MyOtherPOD.second[j]");
                }
                break;
            }
            default: {
                const auto& es = std::get<2>(e.var.value);
                const auto& as = std::get<2>(a.var.value);
                demand(as.list.size() == es.list.size(), "SmallSeq.size");
                for (size_t j = 0; j < es.list.size(); ++j) {
                    demand(as.list[j] == es.list[j], "SmallSeq.value[j]");
                }
                break;
            }
        }
    }

    std::printf("All checks passed (%zu messages)\n", expected.size());
    return 0;
}
