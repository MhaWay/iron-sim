#pragma once
// Minimal read-only JSON scanner for the parity test vectors. Flat shapes
// only (object/array/string/number/true/false/null). Preserves member order
// (vector of pairs) — deterministic and std::map-free per the determinism
// rules. Test-only.
#include <cstdint>
#include <cstdlib>
#include <optional>
#include <string>
#include <string_view>
#include <variant>
#include <vector>

namespace jsonlite {

struct Value;
using Array = std::vector<Value>;
using Object = std::vector<std::pair<std::string, Value>>;

struct Value {
    std::variant<std::nullptr_t, std::string, double, Array, Object> v{nullptr};

    bool is_num() const noexcept { return std::holds_alternative<double>(v); }
    bool is_str() const noexcept { return std::holds_alternative<std::string>(v); }
    bool is_arr() const noexcept { return std::holds_alternative<Array>(v); }
    bool is_obj() const noexcept { return std::holds_alternative<Object>(v); }

    double num() const noexcept { return std::get<double>(v); }
    const std::string& str() const noexcept { return std::get<std::string>(v); }
    const Array& arr() const noexcept { return std::get<Array>(v); }
    const Object& obj() const noexcept { return std::get<Object>(v); }

    int64_t i64() const noexcept { return static_cast<int64_t>(num()); }
};

inline const Value* find(const Value& v, std::string_view key) noexcept {
    if (!v.is_obj()) return nullptr;
    for (const auto& [k, val] : v.obj())
        if (k == key) return &val;
    return nullptr;
}

class Parser {
public:
    explicit Parser(std::string_view s) noexcept : s_(s) {}

    std::optional<Value> parse() {
        skip_ws();
        if (s_.empty()) return std::nullopt;
        return value();
    }

private:
    void skip_ws() noexcept {
        while (!s_.empty() && (s_[0] == ' ' || s_[0] == '\n' || s_[0] == '\t' || s_[0] == '\r'))
            s_.remove_prefix(1);
    }
    bool eat(char c) noexcept {
        skip_ws();
        if (!s_.empty() && s_[0] == c) { s_.remove_prefix(1); return true; }
        return false;
    }

    std::optional<Value> value() {
        skip_ws();
        if (s_.empty()) return std::nullopt;
        switch (s_[0]) {
            case '{': return object();
            case '[': return array();
            case '"': return string();
            case 't': s_.remove_prefix(4); return Value{std::string("true")};
            case 'f': s_.remove_prefix(5); return Value{std::string("false")};
            case 'n': s_.remove_prefix(4); return Value{nullptr};
            default: return number();
        }
    }

    std::optional<Value> object() {
        if (!eat('{')) return std::nullopt;
        Object o;
        while (!s_.empty() && s_[0] != '}') {
            auto k = value();
            if (!k) return std::nullopt;
            if (!eat(':')) return std::nullopt;
            auto v = value();
            if (!v) return std::nullopt;
            o.emplace_back(k->is_str() ? k->str() : std::string(), std::move(*v));
            if (!eat(',')) break;
        }
        if (!eat('}')) return std::nullopt;
        return Value{std::move(o)};
    }

    std::optional<Value> array() {
        if (!eat('[')) return std::nullopt;
        Array a;
        while (!s_.empty() && s_[0] != ']') {
            auto v = value();
            if (!v) return std::nullopt;
            a.push_back(std::move(*v));
            if (!eat(',')) break;
        }
        if (!eat(']')) return std::nullopt;
        return Value{std::move(a)};
    }

    std::optional<Value> string() {
        if (!eat('"')) return std::nullopt;
        std::string out;
        while (!s_.empty() && s_[0] != '"') {
            char c = s_[0];
            s_.remove_prefix(1);
            if (c == '\\') {
                if (s_.empty()) return std::nullopt;
                const char e = s_[0];
                s_.remove_prefix(1);
                switch (e) {
                    case '"': out.push_back('"'); break;
                    case '\\': out.push_back('\\'); break;
                    case '/': out.push_back('/'); break;
                    case 'n': out.push_back('\n'); break;
                    case 't': out.push_back('\t'); break;
                    case 'r': out.push_back('\r'); break;
                    case 'b': out.push_back('\b'); break;
                    case 'f': out.push_back('\f'); break;
                    case 'u': {
                        if (s_.size() < 4) return std::nullopt;
                        std::string hex(s_.substr(0, 4));
                        s_.remove_prefix(4);
                        const long cp = std::strtol(hex.c_str(), nullptr, 16);
                        if (cp > 0 && cp < 0x80) out.push_back(static_cast<char>(cp));
                        break;
                    }
                    default: return std::nullopt;
                }
            } else {
                out.push_back(c);
            }
        }
        if (!eat('"')) return std::nullopt;
        return Value{std::move(out)};
    }

    std::optional<Value> number() {
        skip_ws();
        std::size_t i = 0;
        if (i < s_.size() && (s_[i] == '-' || s_[i] == '+')) ++i;
        const std::size_t digits_start = i;
        while (i < s_.size() && s_[i] >= '0' && s_[i] <= '9') ++i;
        bool is_int = (i > digits_start);
        if (i < s_.size() && (s_[i] == '.' || s_[i] == 'e' || s_[i] == 'E')) is_int = false;
        const std::string tok(s_.substr(0, i));
        s_.remove_prefix(i);
        if (is_int) return Value{static_cast<double>(std::strtoll(tok.c_str(), nullptr, 10))};
        return Value{std::strtod(tok.c_str(), nullptr)};
    }

    std::string_view s_;
};

inline std::optional<Value> parse(std::string_view s) { return Parser(s).parse(); }

} // namespace jsonlite
