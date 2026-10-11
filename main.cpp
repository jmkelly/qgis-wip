//! qgis_oracle_harness — minimal C++ stand-in for a real QGIS binary.
//!
//! Bead qgis-uic.7: a full QGIS build is impractical here (no system
//! cmake, 30-60+ minute compile), so this harness emits the same
//! canonical output format as the Rust oracle backend
//! (crates/oracle/src/rust_backend.rs) with the backend
//! self-identification line set to "cpp". The oracle strips the
//! backend line before comparing, so only fixture-derived content
//! (name, bytes, lines, body) can drift.
//!
//! At E1+ this file grows real QGIS algorithm dispatch; until then it
//! proves the QGIS_CPP_BINARY pipeline end to end.
//!
//! Interface (matches the Rust backend's runner contract):
//!   qgis_oracle_harness <fixture-path>
//! Prints the canonical output to stdout and exits 0; exits 2 with a
//! stderr message on usage or file-read errors.

#include <cstdio>
#include <cstdlib>
#include <string>

namespace {

// Reads the whole file at `path`. Exits with code 2 on failure.
std::string read_file(const char* path) {
    std::FILE* f = std::fopen(path, "rb");
    if (f == nullptr) {
        std::fprintf(stderr, "qgis_oracle_harness: cannot open %s\n", path);
        std::exit(2);
    }
    std::string content;
    char buf[8192];
    std::size_t n = 0;
    while ((n = std::fread(buf, 1, sizeof buf, f)) > 0) {
        content.append(buf, n);
    }
    std::fclose(f);
    return content;
}

// Last path component, mirroring Rust's Path::file_name (Unix '/' split).
std::string file_name(const std::string& path) {
    const std::size_t slash = path.find_last_of("/\\");
    return slash == std::string::npos ? path : path.substr(slash + 1);
}

// Counts lines the way Rust's str::lines() does: split on '\n'; a single
// trailing '\n' does not start a new line.
std::size_t count_lines(const std::string& content) {
    if (content.empty()) {
        return 0;
    }
    std::size_t lines = 1;
    for (const char c : content) {
        if (c == '\n') {
            ++lines;
        }
    }
    if (content.back() == '\n') {
        --lines;
    }
    return lines;
}

// Trims trailing '\n' and '\r' bytes, matching Rust's
// trim_end_matches(['\n', '\r']).
std::string trim_trailing_eol(const std::string& content) {
    std::size_t end = content.size();
    while (end > 0 && (content[end - 1] == '\n' || content[end - 1] == '\r')) {
        --end;
    }
    return content.substr(0, end);
}

}  // namespace

int main(int argc, char** argv) {
    if (argc != 2) {
        std::fprintf(stderr, "usage: qgis_oracle_harness <fixture-path>\n");
        return 2;
    }

    const std::string content = read_file(argv[1]);
    const std::string name = file_name(argv[1]);
    const std::size_t bytes = content.size();
    const std::size_t lines = count_lines(content);
    const std::string body = trim_trailing_eol(content);

    std::printf(
        "backend: cpp/0.1.0\nfixture: %s\nbytes: %zu\nlines: %zu\n---\n",
        name.c_str(), bytes, lines);
    if (!body.empty()) {
        std::fwrite(body.data(), 1, body.size(), stdout);
        std::fputc('\n', stdout);
    }
    return 0;
}
