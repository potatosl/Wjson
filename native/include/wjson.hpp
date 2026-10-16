// SPDX-License-Identifier: MPL-2.0
//
// wjson.hpp — header-only C++20 view over the wjson C ABI, so C++ programs
// can use the same parser (Rust core) and conversions (native) as Wave.
//
//   auto doc = wjson::Document::parse(R"({"a":[1,2,3]})");
//   if (!doc) print(doc.error().message);
//   for (wjson::Value v : doc.root()["a"]) total += v.as_int().value_or(0);
//
// With a C++ standard library the vocabulary types are the standard ones
// (wjson::Str = std::string_view, wjson::Opt<T> = std::optional<T>,
// wjson::String = std::string). Without one — a freestanding build, e.g.
// clang with no MSVC or MinGW headers — small built-in equivalents with the
// same member names are used instead. Define WJSON_NO_STL to force those.

#ifndef WJSON_HPP
#define WJSON_HPP

#include "wjson.h"
#include "wjson_native.h"

#if !defined(WJSON_NO_STL) && defined(__has_include)
#if __has_include(<string_view>) && __has_include(<optional>) && __has_include(<string>)
#define WJSON_STL 1
#endif
#endif
#ifndef WJSON_STL
#define WJSON_STL 0
#endif

#if WJSON_STL
#include <optional>
#include <string>
#include <string_view>

namespace wjson {
using Str = std::string_view;
template <class T>
using Opt = std::optional<T>;
using String = std::string;
}  // namespace wjson

#else  // freestanding vocabulary types

extern "C" void *malloc(__SIZE_TYPE__ size);
extern "C" void free(void *p);

namespace wjson {

typedef __SIZE_TYPE__ size_t;

// Non-owning view of bytes (like std::string_view).
class Str {
public:
    typedef size_t size_type;
    constexpr Str() : p_(""), n_(0) {}
    constexpr Str(const char *p, size_t n) : p_(p), n_(n) {}
    Str(const char *s) : p_(s), n_(0) {  // NOLINT: implicit like string_view
        while (s[n_]) ++n_;
    }
    constexpr const char *data() const { return p_; }
    constexpr size_t size() const { return n_; }
    constexpr bool empty() const { return n_ == 0; }
    constexpr char operator[](size_t i) const { return p_[i]; }
    constexpr const char *begin() const { return p_; }
    constexpr const char *end() const { return p_ + n_; }

    friend bool operator==(Str a, Str b) {
        if (a.n_ != b.n_) return false;
        for (size_t i = 0; i < a.n_; ++i) {
            if (a.p_[i] != b.p_[i]) return false;
        }
        return true;
    }
    friend bool operator!=(Str a, Str b) { return !(a == b); }

private:
    const char *p_;
    size_t n_;
};

// Optional value (like std::optional, for trivially copyable T).
template <class T>
class Opt {
public:
    constexpr Opt() : has_(false), v_() {}
    constexpr Opt(T v) : has_(true), v_(v) {}  // NOLINT: implicit like optional
    constexpr bool has_value() const { return has_; }
    constexpr explicit operator bool() const { return has_; }
    constexpr const T &operator*() const { return v_; }
    constexpr const T *operator->() const { return &v_; }
    constexpr T value_or(T fallback) const { return has_ ? v_ : fallback; }

    friend bool operator==(const Opt &o, const T &v) { return o.has_ && o.v_ == v; }
    friend bool operator!=(const Opt &o, const T &v) { return !(o == v); }

private:
    bool has_;
    T v_;
};

// Owning, move-only byte string (the subset of std::string wjson returns).
class String {
public:
    String() = default;
    explicit String(size_t n) : p_(static_cast<char *>(malloc(n + 1))), n_(p_ ? n : 0) {
        if (p_) p_[n_] = '\0';
    }
    String(const char *p, size_t n) : String(n) {
        for (size_t i = 0; i < n_; ++i) p_[i] = p[i];
    }
    String(String &&o) noexcept : p_(o.p_), n_(o.n_) {
        o.p_ = nullptr;
        o.n_ = 0;
    }
    String &operator=(String &&o) noexcept {
        if (this != &o) {
            free(p_);
            p_ = o.p_;
            n_ = o.n_;
            o.p_ = nullptr;
            o.n_ = 0;
        }
        return *this;
    }
    String(const String &) = delete;
    String &operator=(const String &) = delete;
    ~String() { free(p_); }

    char *data() { return p_ ? p_ : empty_(); }
    const char *data() const { return p_ ? p_ : empty_(); }
    const char *c_str() const { return data(); }
    size_t size() const { return n_; }
    bool empty() const { return n_ == 0; }
    operator Str() const { return Str(data(), n_); }  // NOLINT: implicit like std::string

    friend bool operator==(const String &a, Str b) { return Str(a) == b; }
    friend bool operator!=(const String &a, Str b) { return !(Str(a) == b); }

private:
    static char *empty_() {
        static char zero = '\0';
        return &zero;
    }
    char *p_ = nullptr;
    size_t n_ = 0;
};

}  // namespace wjson

#endif  // WJSON_STL

namespace wjson {

enum class Type : int32_t {
    Null = WJ_NULL,
    Bool = WJ_BOOL,
    Number = WJ_NUMBER,
    String = WJ_STRING,
    Array = WJ_ARRAY,
    Object = WJ_OBJECT,
    Invalid = WJ_INVALID,
};

struct Error {
    int32_t code = 0;
    Str message;
    int64_t offset = 0;
    int64_t line = 0;
    int64_t column = 0;
};

inline Str as_view(const uint8_t *p, int64_t len) {
    return p ? Str(reinterpret_cast<const char *>(p), static_cast<size_t>(len)) : Str();
}

// A cheap, copyable reference to one value inside a Document. Looking up a
// missing key or index yields an Invalid value instead of failing, so chains
// like v["a"]["b"][3] are always safe.
class Value {
public:
    Value() = default;
    Value(const wj_doc *doc, int32_t id) : doc_(doc), id_(id) {}

    Type type() const { return static_cast<Type>(wj_type(doc_, id_)); }
    bool valid() const { return type() != Type::Invalid; }
    explicit operator bool() const { return valid(); }
    int32_t id() const { return id_; }

    bool is_null() const { return type() == Type::Null; }
    bool is_bool() const { return type() == Type::Bool; }
    bool is_number() const { return type() == Type::Number; }
    bool is_string() const { return type() == Type::String; }
    bool is_array() const { return type() == Type::Array; }
    bool is_object() const { return type() == Type::Object; }

    // Elements (array), members (object) or bytes (string); 0 otherwise.
    size_t size() const {
        int64_t n = wj_len(doc_, id_);
        return n < 0 ? 0 : static_cast<size_t>(n);
    }

    Value operator[](Str key) const {
        return {doc_, wj_get(doc_, id_, reinterpret_cast<const uint8_t *>(key.data()),
                             static_cast<int64_t>(key.size()))};
    }
    Value operator[](const char *key) const { return (*this)[Str(key)]; }
    Value operator[](size_t index) const {
        return {doc_, wj_at(doc_, id_, static_cast<int64_t>(index))};
    }
    Value operator[](int index) const { return (*this)[static_cast<size_t>(index)]; }

    // RFC 6901 JSON Pointer relative to this value, e.g. "/users/0/name".
    Value at_pointer(Str pointer) const {
        return {doc_, wj_pointer(doc_, id_, reinterpret_cast<const uint8_t *>(pointer.data()),
                                 static_cast<int64_t>(pointer.size()))};
    }

    Str key_at(size_t index) const {
        int64_t len = 0;
        const uint8_t *p = wj_key(doc_, id_, static_cast<int64_t>(index), &len);
        return as_view(p, len);
    }

    Opt<bool> as_bool() const {
        int32_t b = wj_bool(doc_, id_);
        if (b < 0) return {};
        return b == 1;
    }

    Opt<Str> as_string() const {
        int64_t len = 0;
        const uint8_t *p = wj_str(doc_, id_, &len);
        if (!p) return {};
        return as_view(p, len);
    }

    // Raw lexeme as written in the source ("1e3", "-0.50", ...).
    Opt<Str> number_text() const {
        int64_t len = 0;
        const uint8_t *p = wj_num(doc_, id_, &len);
        if (!p) return {};
        return as_view(p, len);
    }

    Opt<int64_t> as_int() const {
        Opt<Str> text = number_text();
        int64_t v = 0;
        if (!text || wjn_parse_i64(text->data(), static_cast<int64_t>(text->size()), &v) != WJN_OK) return {};
        return v;
    }

    Opt<double> as_double() const {
        Opt<Str> text = number_text();
        double v = 0;
        if (!text || wjn_parse_f64(text->data(), static_cast<int64_t>(text->size()), &v) != WJN_OK) return {};
        return v;
    }

    // Iterates array elements or object member values.
    class iterator {
    public:
        iterator(const Value *owner, size_t i) : owner_(owner), i_(i) {}
        Value operator*() const { return (*owner_)[i_]; }
        iterator &operator++() {
            ++i_;
            return *this;
        }
        bool operator!=(const iterator &o) const { return i_ != o.i_; }
        size_t index() const { return i_; }

    private:
        const Value *owner_;
        size_t i_;
    };
    iterator begin() const { return {this, 0}; }
    iterator end() const { return {this, is_array() || is_object() ? size() : 0}; }

private:
    const wj_doc *doc_ = nullptr;
    int32_t id_ = -1;
};

struct ParseOptions {
    int32_t max_depth = 0;  // 0 = library default (512)
    bool trailing_commas = false;
    bool comments = false;
};

// Owns a parsed document. Move-only.
class Document {
public:
    static Document parse(Str text, ParseOptions opts = {}) {
        uint32_t flags = (opts.trailing_commas ? uint32_t{WJ_ALLOW_TRAILING_COMMAS} : 0u) |
                         (opts.comments ? uint32_t{WJ_ALLOW_COMMENTS} : 0u);
        return Document(wj_parse_ex(reinterpret_cast<const uint8_t *>(text.data()),
                                    static_cast<int64_t>(text.size()), opts.max_depth, flags));
    }

    Document(Document &&o) noexcept : doc_(o.doc_) { o.doc_ = nullptr; }
    Document &operator=(Document &&o) noexcept {
        if (this != &o) {
            wj_free(doc_);
            doc_ = o.doc_;
            o.doc_ = nullptr;
        }
        return *this;
    }
    Document(const Document &) = delete;
    Document &operator=(const Document &) = delete;
    ~Document() { wj_free(doc_); }

    bool ok() const { return wj_ok(doc_) == 1; }
    explicit operator bool() const { return ok(); }
    Value root() const { return {doc_, wj_root(doc_)}; }
    size_t node_count() const { return static_cast<size_t>(wj_node_count(doc_)); }

    Error error() const {
        const char *msg = reinterpret_cast<const char *>(wj_error_message(doc_));
        return {wj_error_code(doc_), msg ? Str(msg) : Str(), wj_error_offset(doc_), wj_error_line(doc_),
                wj_error_column(doc_)};
    }

    const wj_doc *handle() const { return doc_; }

private:
    explicit Document(wj_doc *doc) : doc_(doc) {}
    wj_doc *doc_ = nullptr;
};

// Convenience wrappers over the native conversions.
inline String escape(Str s, uint32_t flags = WJN_ESCAPE_QUOTES) {
    int64_t need = wjn_escape(s.data(), static_cast<int64_t>(s.size()), nullptr, 0, flags);
    if (need < 0) return String();
#if WJSON_STL
    String out(static_cast<size_t>(need), '\0');
#else
    String out(static_cast<size_t>(need));
    if (out.size() != static_cast<size_t>(need)) return String();  // malloc failed
#endif
    wjn_escape(s.data(), static_cast<int64_t>(s.size()), out.data(), need + 1, flags);
    return out;
}

inline String format_double(double v) {
    char buf[64];
    int64_t n = wjn_format_f64(v, buf, sizeof buf);
    if (n < 0) return String("null", 4);
    return String(buf, static_cast<size_t>(n));
}

}  // namespace wjson

#endif  // WJSON_HPP
