#pragma once
#include <array>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <span>
#include <type_traits>
#include <variant>
#include <utility>

// Reader concept: bool read_bytes(std::byte*, size_t);
// Reader concept: std::span<std::byte> advance_bytes(size_t);
// Writer concept: bool write_bytes(std::byte const*, size_t);
namespace example {
    template <class T> struct ArrayRef;
    namespace readers 
    {
        enum class PlainEnumReader : std::uint8_t;
        
        enum class BetterEnumReader : std::uint8_t;
        
        using ShortStringReader = ArrayRef<std::uint8_t>;
        
        using FixedStringReader = std::array<std::uint8_t, 4>;
        
        struct MyPODReader;
        
        using DataReader = ArrayRef<float>;
        
        struct MyFlagsReader;
        
        struct MyOtherPODReader;
        
        struct SmallSeqReader;
        
        using AlphaReader = MyOtherPODReader;
        
        struct MyVariantReader;
        
        struct RootReader;
        
        template <class Reader> bool read(Reader&, PlainEnumReader&);
        template <class Reader> bool read(Reader&, BetterEnumReader&);
        template <class Reader> bool read(Reader&, ShortStringReader&);
        template <class Reader> bool read(Reader&, FixedStringReader&);
        template <class Reader> bool read(Reader&, MyPODReader&);
        template <class Reader> bool read(Reader&, DataReader&);
        template <class Reader> bool read(Reader&, MyFlagsReader&);
        template <class Reader> bool read(Reader&, MyOtherPODReader&);
        template <class Reader> bool read(Reader&, SmallSeqReader&);
        template <class Reader> bool read(Reader&, MyVariantReader&);
        template <class Reader> bool read(Reader&, RootReader&);
    }
    
    namespace writers 
    {
        enum class PlainEnumWriter : std::uint8_t;
        
        enum class BetterEnumWriter : std::uint8_t;
        
        using ShortStringWriter = std::span<std::uint8_t>;
        
        using FixedStringWriter = std::array<std::uint8_t, 4>;
        
        struct MyPODWriter;
        
        using DataWriter = std::span<float>;
        
        struct MyFlagsWriter;
        
        struct MyOtherPODWriter;
        
        struct SmallSeqWriter;
        
        using AlphaWriter = MyOtherPODWriter;
        
        struct MyVariantWriter;
        
        struct RootWriter;
        
        template <class Writer> bool write(Writer&, PlainEnumWriter const&);
        template <class Writer> bool write(Writer&, BetterEnumWriter const&);
        template <class Writer> bool write(Writer&, ShortStringWriter const&);
        template <class Writer> bool write(Writer&, FixedStringWriter const&);
        template <class Writer> bool write(Writer&, MyPODWriter const&);
        template <class Writer> bool write(Writer&, DataWriter const&);
        template <class Writer> bool write(Writer&, MyFlagsWriter const&);
        template <class Writer> bool write(Writer&, MyOtherPODWriter const&);
        template <class Writer> bool write(Writer&, SmallSeqWriter const&);
        template <class Writer> bool write(Writer&, MyVariantWriter const&);
        template <class Writer> bool write(Writer&, RootWriter const&);
    }
    
    template <class Reader>
    inline bool read_bytes(Reader& reader, void* dst, size_t n)
    {
        return reader.read_bytes(reinterpret_cast<std::byte*>(dst), n);
    }
    
    template <class Writer>
    inline bool write_bytes(Writer& writer, void const* src, size_t n)
    {
        return writer.write_bytes(reinterpret_cast<std::byte const*>(src), n);
    }
    
    template <class Reader, class T>
    inline bool read_scalar(Reader& reader, T& value)
    {
        static_assert(std::is_trivially_copyable_v<T>);
        return read_bytes(reader, &value, sizeof(T));
    }
    
    template <class Writer, class T>
    inline bool write_scalar(Writer& writer, T const& value)
    {
        static_assert(std::is_trivially_copyable_v<T>);
        return write_bytes(writer, &value, sizeof(T));
    }
    
    template <class Reader, class T>
    inline bool read_value(Reader& reader, T& value)
    {
        if constexpr (std::is_trivially_copyable_v<T> && std::is_arithmetic_v<T>)
        {
            return read_scalar(reader, value);
        }
        else
        {
            return readers::read(reader, value);
        }
    }
    
    template <class Writer, class T>
    inline bool write_value(Writer& writer, T const& value)
    {
        if constexpr (std::is_trivially_copyable_v<T> && std::is_arithmetic_v<T>)
        {
            return write_scalar(writer, value);
        }
        else
        {
            return writers::write(writer, value);
        }
    }
    
    template <class T>
    struct ArrayRef
    {
        const std::byte* content_ptr;
        size_t t_count;
        void copy_to(std::span<T> dest)
        {
            auto count = std::min(dest.size(), t_count);
            std::memcpy(dest.data(), content_ptr, sizeof(T)*count);
        }
        void copy_to_vector(std::vector<T>& dest)
        {
            dest.resize(t_count);
            std::memcpy(dest.data(), content_ptr, sizeof(T)*t_count);
        }
        template <class Function> void for_each(Function&& f)
        {
            T temp;
            for (size_t i = 0; i < t_count; i++)
            {
                std::memcpy(&temp, content_ptr + (i * sizeof(T)), sizeof(T));
                f(i, temp);
            }
        }
    }
    ;
    
    namespace readers 
    {
        enum class PlainEnumReader : std::uint8_t 
        {
            F1 = 0,
            F2 = 1,
        }
        ;
        
        enum class BetterEnumReader : std::uint8_t 
        {
            A = 0,
            B = 1,
            DEFAULT = 255,
        }
        ;
        
        using ShortStringReader = ArrayRef<std::uint8_t>;
        
        using FixedStringReader = std::array<std::uint8_t, 4>;
        
        #pragma pack(push, 1)
        struct MyPODReader
        {
            std::uint8_t a_thing;
            std::uint64_t b_thing;
        }
        ;
        #pragma pack(pop)
        static_assert(std::is_trivially_copyable_v<MyPODReader>, "pack must be POD");
        
        using DataReader = ArrayRef<float>;
        
        struct MyFlagsReader 
        {
            std::uint8_t is_thing{};
            std::uint8_t another_thing{};
            PlainEnumReader some_stuff{};
        }
        ;
        
        #pragma pack(push, 1)
        struct MyOtherPODReader
        {
            MyPODReader first;
            FixedStringReader second;
        }
        ;
        #pragma pack(pop)
        static_assert(std::is_trivially_copyable_v<MyOtherPODReader>, "pack must be POD");
        
        struct SmallSeqReader 
        {
            DataReader  list;
        }
        ;
        
        using AlphaReader = MyOtherPODReader;
        
        struct MyVariantReader : std::variant<MyPODReader, MyOtherPODReader, std::monostate, SmallSeqReader>
        {
            using variant::variant;
            using variant::operator=;
        }
        ;
        
        struct RootReader 
        {
            ShortStringReader  name;
            MyVariantReader  var;
        }
        ;
        
        
        template <class Reader> bool read(Reader&, PlainEnumReader&);
        template <class Reader> bool read(Reader&, BetterEnumReader&);
        template <class Reader> bool read(Reader&, ShortStringReader&);
        template <class Reader> bool read(Reader&, FixedStringReader&);
        template <class Reader> bool read(Reader&, MyPODReader&);
        template <class Reader> bool read(Reader&, DataReader&);
        template <class Reader> bool read(Reader&, MyFlagsReader&);
        template <class Reader> bool read(Reader&, MyOtherPODReader&);
        template <class Reader> bool read(Reader&, SmallSeqReader&);
        template <class Reader> bool read(Reader&, MyVariantReader&);
        template <class Reader> bool read(Reader&, RootReader&);
        
        template <class Reader> inline bool read(Reader& reader, PlainEnumReader& value)
        {
            std::uint8_t raw{};
            if (!read_scalar(reader, raw)) return false;
            switch (raw)
            {
                case 0: value = PlainEnumReader::F1; return true;
                case 1: value = PlainEnumReader::F2; return true;
                default: return false;
            }
        }
        template <class Reader> inline bool read(Reader& reader, BetterEnumReader& value)
        {
            std::uint8_t raw{};
            if (!read_scalar(reader, raw)) return false;
            switch (raw)
            {
                case 0: value = BetterEnumReader::A; return true;
                case 1: value = BetterEnumReader::B; return true;
                default: value = BetterEnumReader::DEFAULT; return true;
            }
        }
        template <class Reader> inline bool read(Reader& reader, ShortStringReader& value)
        {
            std::uint8_t count_raw{};
            if (!read_scalar(reader, count_raw)) return false;
            auto count = static_cast<uint64_t>(count_raw);
            if (count > static_cast<uint64_t>(std::numeric_limits<size_t>::max())) return false;
            auto byte_count = sizeof(std::uint8_t) * static_cast<size_t>(count);
            auto ptr = reader.advance_bytes(byte_count);
            if (ptr.empty() && count > 0) return false;
            value.content_ptr = ptr.data(); value.t_count = count;
            return true;
        }
        template <class Reader> inline bool read(Reader& reader, FixedStringReader& value)
        {
            return read_scalar(reader, value);
        }
        template <class Reader> inline bool read(Reader& reader, MyPODReader& value)
        {
            return read_scalar(reader, value);
        }
        template <class Reader> inline bool read(Reader& reader, DataReader& value)
        {
            std::uint8_t count_raw{};
            if (!read_scalar(reader, count_raw)) return false;
            auto count = static_cast<uint64_t>(count_raw);
            if (count > static_cast<uint64_t>(std::numeric_limits<size_t>::max())) return false;
            auto byte_count = sizeof(float) * static_cast<size_t>(count);
            auto ptr = reader.advance_bytes(byte_count);
            if (ptr.empty() && count > 0) return false;
            value.content_ptr = ptr.data(); value.t_count = count;
            return true;
        }
        template <class Reader> inline bool read(Reader& reader, MyFlagsReader& value)
        {
            std::uint8_t raw{};
            if (!read_scalar(reader, raw)) return false;
            auto bits = static_cast<uint64_t>(raw);
            value.is_thing = static_cast<std::uint8_t>((bits >> 0) & 0x1ull);
            value.another_thing = static_cast<std::uint8_t>((bits >> 1) & 0x3ull);
            value.some_stuff = static_cast<PlainEnumReader>((bits >> 3) & 0x3ull);
            return true;
        }
        template <class Reader> inline bool read(Reader& reader, MyOtherPODReader& value)
        {
            return read_scalar(reader, value);
        }
        template <class Reader> inline bool read(Reader& reader, SmallSeqReader& value)
        {
            if (!read_value(reader, value.list)) return false;
            return true;
        }
        template <class Reader> inline bool read(Reader& reader, MyVariantReader& value)
        {
            std::uint8_t tag{};
            if (!read_scalar(reader, tag)) return false;
            switch (tag)
            {
                case 1:
                {
                    MyPODReader payload{};
                    if (!read_value(reader, payload)) return false;
                    value.emplace<0>(payload);
                    return true;
                }
                case 2:
                {
                    MyOtherPODReader payload{};
                    if (!read_value(reader, payload)) return false;
                    value.emplace<1>(payload);
                    return true;
                }
                case 3:
                {
                    value = std::monostate{};
                    return true;
                }
                case 4:
                {
                    SmallSeqReader payload{};
                    if (!read_value(reader, payload)) return false;
                    value.emplace<3>(payload);
                    return true;
                }
                default: return false;
            }
        }
        template <class Reader> inline bool read(Reader& reader, RootReader& value)
        {
            if (!read_value(reader, value.name)) return false;
            if (!read_value(reader, value.var)) return false;
            return true;
        }
    }
    
    namespace writers 
    {
        enum class PlainEnumWriter : std::uint8_t 
        {
            F1 = 0,
            F2 = 1,
        }
        ;
        
        enum class BetterEnumWriter : std::uint8_t 
        {
            A = 0,
            B = 1,
            DEFAULT = 255,
        }
        ;
        
        using ShortStringWriter = std::span<std::uint8_t>;
        
        using FixedStringWriter = std::array<std::uint8_t, 4>;
        
        #pragma pack(push, 1)
        struct MyPODWriter
        {
            std::uint8_t a_thing;
            std::uint64_t b_thing;
        }
        ;
        #pragma pack(pop)
        static_assert(std::is_trivially_copyable_v<MyPODWriter>, "pack must be POD");
        
        using DataWriter = std::span<float>;
        
        struct MyFlagsWriter 
        {
            std::uint8_t is_thing{};
            std::uint8_t another_thing{};
            PlainEnumWriter some_stuff{};
        }
        ;
        
        #pragma pack(push, 1)
        struct MyOtherPODWriter
        {
            MyPODWriter first;
            FixedStringWriter second;
        }
        ;
        #pragma pack(pop)
        static_assert(std::is_trivially_copyable_v<MyOtherPODWriter>, "pack must be POD");
        
        struct SmallSeqWriter 
        {
            DataWriter const& list;
        }
        ;
        
        using AlphaWriter = MyOtherPODWriter;
        
        struct MyVariantWriter : std::variant<MyPODWriter const*, MyOtherPODWriter const*, std::monostate, SmallSeqWriter const*>
        {
            using variant::variant;
            using variant::operator=;
        }
        ;
        
        struct RootWriter 
        {
            ShortStringWriter const& name;
            MyVariantWriter const& var;
        }
        ;
        
        
        template <class Writer> bool write(Writer&, PlainEnumWriter const&);
        template <class Writer> bool write(Writer&, BetterEnumWriter const&);
        template <class Writer> bool write(Writer&, ShortStringWriter const&);
        template <class Writer> bool write(Writer&, FixedStringWriter const&);
        template <class Writer> bool write(Writer&, MyPODWriter const&);
        template <class Writer> bool write(Writer&, DataWriter const&);
        template <class Writer> bool write(Writer&, MyFlagsWriter const&);
        template <class Writer> bool write(Writer&, MyOtherPODWriter const&);
        template <class Writer> bool write(Writer&, SmallSeqWriter const&);
        template <class Writer> bool write(Writer&, MyVariantWriter const&);
        template <class Writer> bool write(Writer&, RootWriter const&);
        
        template <class Writer> inline bool write(Writer& writer, PlainEnumWriter const& value)
        {
            auto raw = static_cast<std::uint8_t>(value);
            return write_scalar(writer, raw);
        }
        template <class Writer> inline bool write(Writer& writer, BetterEnumWriter const& value)
        {
            auto raw = static_cast<std::uint8_t>(value);
            return write_scalar(writer, raw);
        }
        template <class Writer> inline bool write(Writer& writer, ShortStringWriter const& value)
        {
            auto count = value.size();
            if (count > static_cast<size_t>(std::numeric_limits<uint8_t>::max())) return false;
            std::uint8_t count_raw = static_cast<std::uint8_t>(count);
            if (!write_scalar(writer, count_raw)) return false;
            for (auto const& elem : value)
            {
                if (!write_value(writer, elem)) return false;
            }
            return true;
        }
        template <class Writer> inline bool write(Writer& writer, FixedStringWriter const& value)
        {
            for (auto const& elem : value)
            {
                if (!write_value(writer, elem)) return false;
            }
            return true;
        }
        template <class Writer> inline bool write(Writer& writer, MyPODWriter const& value)
        {
            return write_scalar(writer, value);
        }
        template <class Writer> inline bool write(Writer& writer, DataWriter const& value)
        {
            auto count = value.size();
            if (count > static_cast<size_t>(std::numeric_limits<uint8_t>::max())) return false;
            std::uint8_t count_raw = static_cast<std::uint8_t>(count);
            if (!write_scalar(writer, count_raw)) return false;
            for (auto const& elem : value)
            {
                if (!write_value(writer, elem)) return false;
            }
            return true;
        }
        template <class Writer> inline bool write(Writer& writer, MyFlagsWriter const& value)
        {
            uint64_t raw = 0;
            raw |= (static_cast<uint64_t>(value.is_thing) & 0x1ull) << 0;
            raw |= (static_cast<uint64_t>(value.another_thing) & 0x3ull) << 1;
            raw |= (static_cast<uint64_t>(value.some_stuff) & 0x3ull) << 3;
            std::uint8_t out = static_cast<std::uint8_t>(raw);
            return write_scalar(writer, out);
        }
        template <class Writer> inline bool write(Writer& writer, MyOtherPODWriter const& value)
        {
            return write_scalar(writer, value);
        }
        template <class Writer> inline bool write(Writer& writer, SmallSeqWriter const& value)
        {
            if (!write_value(writer, value.list)) return false;
            return true;
        }
        template <class Writer> inline bool write(Writer& writer, MyVariantWriter const& value)
        {
            switch (value.index())
            {
                case 0: 
                {
                    auto payload = std::get<0>(value);
                    if (payload == nullptr) return false;
                    auto tag = static_cast<std::uint8_t>(1);
                    if (!write_scalar(writer, tag)) return false;
                    return write_value(writer, *payload);
                }
                case 1: 
                {
                    auto payload = std::get<1>(value);
                    if (payload == nullptr) return false;
                    auto tag = static_cast<std::uint8_t>(2);
                    if (!write_scalar(writer, tag)) return false;
                    return write_value(writer, *payload);
                }
                case 2: 
                {
                    auto tag = static_cast<std::uint8_t>(3);
                    if (!write_scalar(writer, tag)) return false;
                    return true;
                }
                case 3: 
                {
                    auto payload = std::get<3>(value);
                    if (payload == nullptr) return false;
                    auto tag = static_cast<std::uint8_t>(4);
                    if (!write_scalar(writer, tag)) return false;
                    return write_value(writer, *payload);
                }
                default: return false;
            }
        }
        template <class Writer> inline bool write(Writer& writer, RootWriter const& value)
        {
            if (!write_value(writer, value.name)) return false;
            if (!write_value(writer, value.var)) return false;
            return true;
        }
    }
}
