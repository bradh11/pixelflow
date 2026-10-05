#!/bin/sh
# Optional check: does FPP's own .fseq reader decode these files exactly as PixelFlow's does?
#
#   scripts/fseq-fpp-oracle.sh file.fseq [more.fseq ...]
#
# Fetches FPP's fseq reader (src/fseq/FSEQFile.cpp, read-only, from GitHub) into a scratch
# directory outside the repo, builds a small harness against it, and for each file compares
# FPP's view (header, every frame's FNV-1a checksum, in fppd's streaming and bulk read modes)
# with `cargo run -p pf-fseq --example fseq_dump`. Exits non-zero on any difference.
#
# FPP's code is GPL/LGPL: it is only fetched and built in the scratch directory, never copied
# into this repository. Needs git, a C++17 compiler, zstd (headers + library) and zlib.
#
# Environment: FPP_ORACLE_DIR (scratch directory, default $TMPDIR/pixelflow-fpp-oracle),
# FPP_REF (FPP branch or tag to fetch, default master).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=${FPP_ORACLE_DIR:-${TMPDIR:-/tmp}/pixelflow-fpp-oracle}
REF=${FPP_REF:-master}
if [ "$#" -eq 0 ]; then
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi
mkdir -p "$WORK/stubs"

if [ ! -d "$WORK/fpp/.git" ]; then
    git clone -q --depth 1 --branch "$REF" --filter=blob:none --sparse \
        https://github.com/FalconChristmas/fpp.git "$WORK/fpp"
    git -C "$WORK/fpp" sparse-checkout set src/fseq
fi
echo "FPP $(git -C "$WORK/fpp" log -1 --format='%h (%cs)')"

# Stand-ins for the parts of FPP the reader includes (logging, warnings).
cat > "$WORK/stubs/log.h" <<'EOF'
#pragma once
#include <cstdio>
#define VB_SEQUENCE 1
#define VB_ALL 0
#define LogErr(lvl, ...) do { fprintf(stderr, "FPP: "); fprintf(stderr, __VA_ARGS__); } while (0)
#define LogWarn(lvl, ...) LogErr(lvl, __VA_ARGS__)
#define LogInfo(lvl, ...) do {} while (0)
#define LogDebug(lvl, ...) do {} while (0)
#define LogExcess(lvl, ...) do {} while (0)
EOF
cat > "$WORK/stubs/Warnings.h" <<'EOF'
#pragma once
#include <string>
struct WarningHolder { static void AddWarningTimeout(int, int, const std::string&) {} };
EOF
# Opens a file as fppd does, reads every frame through getFrame()/readFrame() and prints the
# same lines as pf-fseq's fseq_dump example.
cat > "$WORK/harness.cpp" <<'EOF'
#include "FSEQFile.h"
#include <cinttypes>
#include <cstring>
#include <memory>
#include <string>
#include <vector>
int main(int argc, char** argv) {
    bool bulk = argc > 2 && !strcmp(argv[1], "--bulk");
    std::unique_ptr<FSEQFile> f(FSEQFile::openFSEQFile(argv[argc - 1]));
    if (!f) { printf("OPEN FAILED\n"); return 1; }
    printf("version %d.%d\nmax_channel %u\nframes %u\nstep_ms %d\n", f->getVersionMajor(),
           f->getVersionMinor(), f->getMaxChannel(), f->getNumFrames(), f->getStepTime());
    auto* v2 = dynamic_cast<V2FSEQFile*>(f.get());
    printf("compression %s\n", v2 ? v2->CompressionTypeString().c_str() : "none");
    printf("media \"%s\"\n", f->getMediaFilename().c_str());
    std::string producer;
    for (auto& h : f->getVariableHeaders())
        if (h.code[0] == 's' && h.code[1] == 'p' && !h.getData().empty())
            producer.assign((const char*)h.getData().data(), strnlen((const char*)h.getData().data(), h.getData().size()));
    printf("producer \"%s\"\n", producer.c_str());
    if (v2) printf("# fpp: stored %u, blocks %zu, sparse ranges %zu\n", f->getChannelCount(),
                   v2->m_frameOffsets.size() - 1, v2->m_sparseRanges.size());
    if (bulk) f->setReadPattern(FSEQFile::ReadPattern::Bulk);
    uint32_t n = f->getMaxChannel(), failed = 0;
    // fppd asks for the channels its outputs use; here, every channel the file holds.
    std::vector<std::pair<uint32_t, uint32_t>> want{ { 0, n } };
    if (v2 && !v2->m_sparseRanges.empty()) want = v2->m_sparseRanges;
    f->prepareRead(want, 0);
    std::vector<uint8_t> buf(n);
    uint64_t all = 0xcbf29ce484222325ULL;
    for (uint32_t fr = 0; fr < f->getNumFrames(); fr++) {
        std::fill(buf.begin(), buf.end(), 0);
        std::unique_ptr<FSEQFile::FrameData> d(f->getFrame(fr));
        if (!d || !d->readFrame(buf.data(), n)) { printf("frame %u FAILED\n", fr); failed++; continue; }
        uint64_t h = 0xcbf29ce484222325ULL;
        for (uint8_t b : buf) h = (h ^ b) * 0x100000001b3ULL;
        all = (all ^ h) * 0x100000001b3ULL;
        printf("frame %u %016" PRIx64 "\n", fr, h);
    }
    printf("failed_frames %u\nall_frames %016" PRIx64 "\n", failed, all);
    return failed != 0;
}
EOF

ZSTD=$(pkg-config --cflags --libs libzstd 2>/dev/null ||
    { p=$(brew --prefix zstd 2>/dev/null) && echo "-I$p/include -L$p/lib -lzstd"; } || echo "-lzstd")
PLATFORM=-DPLATFORM_UNKNOWN
[ "$(uname)" = Darwin ] && PLATFORM=-DPLATFORM_OSX
# shellcheck disable=SC2086
${CXX:-c++} -std=c++17 -O2 -w $PLATFORM -I"$WORK/stubs" -I"$WORK/fpp/src/fseq" \
    "$WORK/harness.cpp" "$WORK/fpp/src/fseq/FSEQFile.cpp" $ZSTD -lz -lpthread -o "$WORK/fpp-fseq-oracle"
(cd "$ROOT" && cargo build -q --release -p pf-fseq --example fseq_dump)
DUMP=${CARGO_TARGET_DIR:-$ROOT/target}/release/examples/fseq_dump

status=0
for file in "$@"; do
    name=$(basename "$file")
    "$DUMP" "$file" > "$WORK/pf.txt" || true
    for mode in streaming bulk; do
        flag=""
        [ "$mode" = bulk ] && flag=--bulk
        "$WORK/fpp-fseq-oracle" $flag "$file" > "$WORK/fpp.txt" 2> "$WORK/fpp.err" || true
        if grep -v '^#' "$WORK/fpp.txt" | diff - "$WORK/pf.txt" > "$WORK/diff.txt" &&
            ! grep -q 'FAILED' "$WORK/fpp.txt"; then
            echo "MATCH   $name ($mode): $(grep -c '^frame ' "$WORK/fpp.txt") frames"
        else
            echo "DIFFER  $name ($mode):"
            head -n 10 "$WORK/diff.txt"
            status=1
        fi
        [ -s "$WORK/fpp.err" ] && head -n 5 "$WORK/fpp.err"
    done
    grep -E '^(version|max_channel|frames|step_ms|compression|media|producer|#)' "$WORK/fpp.txt" |
        sed 's/^/        /'
done
exit $status
