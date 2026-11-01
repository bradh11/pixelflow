#!/bin/sh
# Helps render the fidelity corpus's references in xLights (crates/pf-fidelity/corpus).
#
#   scripts/fidelity-corpus.sh prepare [folder]            copy the corpus's show to a folder for xLights
#   scripts/fidelity-corpus.sh render <xLights> [folder]   render every sequence there with xLights, headless
#   scripts/fidelity-corpus.sh collect [folder]            bring the .fseq files xLights saved back
#
# `prepare` copies the show folder (layout, networks, sequences, picture, shader) to the folder.
# The sequences name the picture and the shader from the show folder (pictures/..., Shaders/...),
# which is where xLights looks for them. It never touches your own show.
#
# `render` runs xLights 2026.13 or later (the program, or its .app on macOS) with no window on
# that folder: it renders the five sequences, saves each .fseq beside its sequence, and exits.
# It never sends anything to a controller. Or render them in xLights' window (README.md).
#
# `collect` checks that xLights saved a .fseq for every sequence of the corpus, newer than the
# copy `prepare` made, and that the layout in the folder is still the corpus's (xLights renders
# what it has open: a layout changed and saved there would make references of another show), and
# copies the .fseq files into the corpus. Then it says what to run and commit.
#
# The folder is ~/PixelFlow fidelity corpus unless you name one; on macOS it is
# ~/Library/Containers/org.xlights/Data/PixelFlow fidelity corpus, because xLights for macOS is
# sandboxed: run from the command line, it reads only folders in its own container there.
# Nothing here downloads anything. crates/pf-fidelity/corpus/README.md has the steps.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CORPUS="$ROOT/crates/pf-fidelity/corpus"
SEQUENCES="washes chance marks heads words"
CONTAINER="$HOME/Library/Containers/org.xlights/Data"
if [ "$(uname -s)" = Darwin ]; then
    DEFAULT="$CONTAINER/PixelFlow fidelity corpus"
else
    DEFAULT="$HOME/PixelFlow fidelity corpus"
fi

usage() {
    sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
}

prepared() {
    if [ ! -f "$1/.prepared" ]; then
        echo "$1 wasn't made by 'prepare'." >&2
        exit 1
    fi
}

[ "$#" -ge 1 ] || usage

case "$1" in
prepare)
    WORK=${2:-"$DEFAULT"}
    if [ -e "$WORK" ]; then
        echo "$WORK already exists: remove it, or name another folder." >&2
        exit 1
    fi
    mkdir -p "$WORK"
    cp -R "$CORPUS/show/." "$WORK/"
    rm -f "$WORK"/*.fseq
    touch "$WORK/.prepared"
    cat <<EOF
Copied the corpus's show to:
  $WORK

Render it with xLights 2026.13 or later, with no window:
  scripts/fidelity-corpus.sh render <xLights> "$WORK"
(<xLights> is the program, or on macOS its .app, such as /Applications/xLights.app.)

Or in xLights' window (crates/pf-fidelity/corpus/README.md has the details):
  1. Open that folder as the show folder.
  2. Tools > Batch Render: tick washes.xsq, chance.xsq, marks.xsq, heads.xsq, and words.xsq; render.
     (Or open each one, Render All, and File > Save.)
  3. Don't save any change to the layout or the controllers.
  4. Open your own show folder again.
Then:
  scripts/fidelity-corpus.sh collect "$WORK"
EOF
    ;;
render)
    [ "$#" -ge 2 ] || usage
    XLIGHTS=$2
    WORK=${3:-"$DEFAULT"}
    if [ -d "$XLIGHTS" ] && [ -x "$XLIGHTS/Contents/MacOS/xLights" ]; then
        XLIGHTS="$XLIGHTS/Contents/MacOS/xLights"
    fi
    if [ ! -f "$XLIGHTS" ] || [ ! -x "$XLIGHTS" ]; then
        echo "$XLIGHTS isn't a program: name xLights (or, on macOS, xLights.app)." >&2
        exit 1
    fi
    prepared "$WORK"
    case "$XLIGHTS" in
    *.app/Contents/MacOS/*)
        case "$WORK" in
        "$CONTAINER"/*) ;;
        *)
            echo "xLights for macOS reads only folders in $CONTAINER when it runs" >&2
            echo "without its window: prepare a folder there (the default) and render that." >&2
            exit 1
            ;;
        esac
        ;;
    esac
    for name in $SEQUENCES; do
        rm -f "$WORK/$name.fseq"
    done
    set --
    for name in $SEQUENCES; do
        set -- "$@" "$WORK/$name.xsq"
    done
    echo "Rendering the corpus in $WORK with $XLIGHTS ..."
    # --headless: no window, render, save each .fseq to -od, exit. Never -o (output to lights).
    # The alarm stops it should it ever wait for anything.
    if ! perl -e 'alarm 600; exec @ARGV' "$XLIGHTS" --headless -q -s "$WORK" -od "$WORK" "$@"; then
        echo "xLights didn't finish the render. Its log says why: on macOS," >&2
        echo "  $CONTAINER/Library/Logs/xLights_spdlog.log" >&2
        exit 1
    fi
    for name in $SEQUENCES; do
        if [ ! -f "$WORK/$name.fseq" ]; then
            echo "xLights rendered no $name.fseq: its log says why." >&2
            exit 1
        fi
        echo "rendered $name.fseq ($(wc -c < "$WORK/$name.fseq" | tr -d ' ') bytes)"
    done
    cat <<EOF

Now bring them into the corpus:
  scripts/fidelity-corpus.sh collect "$WORK"
EOF
    ;;
collect)
    WORK=${2:-"$DEFAULT"}
    prepared "$WORK"
    for file in xlights_rgbeffects.xml xlights_networks.xml; do
        if ! cmp -s "$CORPUS/show/$file" "$WORK/$file"; then
            echo "The $file in $WORK isn't the corpus's any more: xLights saved a change to it, so" >&2
            echo "its renders aren't of the corpus's show. Run 'prepare' into a new folder and render again" >&2
            echo "without saving the layout." >&2
            exit 1
        fi
    done
    missing=""
    for name in $SEQUENCES; do
        if [ ! -f "$WORK/$name.fseq" ]; then
            missing="$missing $name.fseq"
        elif [ ! "$WORK/$name.fseq" -nt "$WORK/.prepared" ]; then
            missing="$missing $name.fseq(older than the copy)"
        fi
    done
    if [ -n "$missing" ]; then
        echo "xLights hasn't saved a render of every sequence in $WORK:$missing" >&2
        echo "(If xLights keeps its .fseq files in another folder, its Preferences say which: copy them" >&2
        echo "into $WORK and run this again.)" >&2
        exit 1
    fi
    for name in $SEQUENCES; do
        cp "$WORK/$name.fseq" "$CORPUS/show/$name.fseq"
        echo "copied $name.fseq ($(wc -c < "$WORK/$name.fseq" | tr -d ' ') bytes)"
    done
    cat <<EOF

Now make the baseline from them and commit both:
  cargo run --release -p pf-fidelity -- regress --accept
  cargo run --release -p pf-fidelity -- regress
  git add crates/pf-fidelity/corpus/show/*.fseq crates/pf-fidelity/corpus/baseline.json
EOF
    ;;
*)
    usage
    ;;
esac
