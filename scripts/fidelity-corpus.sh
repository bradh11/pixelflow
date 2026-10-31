#!/bin/sh
# Helps render the fidelity corpus's references in xLights (crates/pf-fidelity/corpus).
#
#   scripts/fidelity-corpus.sh prepare [folder]   copy the corpus's show to a folder for xLights
#   scripts/fidelity-corpus.sh collect [folder]   bring the .fseq files xLights saved back
#
# `prepare` copies the show folder (layout, networks, sequences, picture, shader) to the folder
# (default: ~/PixelFlow fidelity corpus), with each sequence's picture and shader named by its
# full path there, so xLights finds them wherever it looks. It never touches your own show.
# Then, in xLights: open that folder as the show folder, render every sequence, save.
#
# `collect` checks that xLights saved a .fseq for every sequence of the corpus, newer than the
# copy `prepare` made, and that the layout in the folder is still the corpus's (xLights renders
# what it has open: a layout changed and saved there would make references of another show), and
# copies the .fseq files into the corpus. Then it says what to run and commit.
#
# Neither starts xLights nor downloads anything. crates/pf-fidelity/corpus/README.md has the
# steps in xLights.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CORPUS="$ROOT/crates/pf-fidelity/corpus"
SEQUENCES="washes chance marks words"
WORK=${2:-"$HOME/PixelFlow fidelity corpus"}

usage() {
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
}

[ "$#" -ge 1 ] || usage

case "$1" in
prepare)
    if [ -e "$WORK" ]; then
        echo "$WORK already exists: remove it, or name another folder." >&2
        exit 1
    fi
    mkdir -p "$WORK"
    cp -R "$CORPUS/show/." "$WORK/"
    rm -f "$WORK"/*.fseq
    for name in $SEQUENCES; do
        # The picture and the shader by their full paths in the folder (the corpus names them
        # from the show folder). The import finds them by name either way.
        sed -e "s#E_FILEPICKERCTRL_Pictures_Filename=pictures/#E_FILEPICKERCTRL_Pictures_Filename=$WORK/pictures/#g" \
            -e "s#E_0FILEPICKERCTRL_IFS=Shaders/#E_0FILEPICKERCTRL_IFS=$WORK/Shaders/#g" \
            "$CORPUS/show/$name.xsq" > "$WORK/$name.xsq"
    done
    touch "$WORK/.prepared"
    cat <<EOF
Copied the corpus's show to:
  $WORK

In xLights (crates/pf-fidelity/corpus/README.md has the details):
  1. Open that folder as the show folder.
  2. Tools > Batch Render: tick washes.xsq, chance.xsq, marks.xsq, and words.xsq; render.
     (Or open each one, Render All, and File > Save.)
  3. Don't save any change to the layout or the controllers.
  4. Open your own show folder again.
Then:
  scripts/fidelity-corpus.sh collect "$WORK"
EOF
    ;;
collect)
    if [ ! -f "$WORK/.prepared" ]; then
        echo "$WORK wasn't made by 'prepare'." >&2
        exit 1
    fi
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
