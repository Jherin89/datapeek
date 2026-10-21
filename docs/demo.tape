# Animated demo of datapeek.
# From the repo root, with ttyd, ffmpeg, jq, and a release build:
#   cargo build --release
#   vhs docs/demo.tape
# Writes docs/demo.gif.

Output docs/demo.gif

Set Shell bash
Set FontFamily "DejaVu Sans Mono"
Set FontSize 15
Set Width 1280
Set Height 440
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 12ms
Set Framerate 8
Set PlaybackSpeed 1
Set WindowBar Colorful
Set CursorBlink false

# `datapeek` and `jq` must be on PATH. From a local build:
#   export PATH="$PWD/target/release:$PATH"
Hide
Type "export PS1='$ '"
Enter
Type "clear"
Enter
Show

Type "datapeek samples/orders.csv"
Enter
Sleep 1.8s

Hide
Type "clear"
Enter
Show

Type "datapeek --columns customer,amount samples/orders.csv"
Enter
Sleep 1.6s

Hide
Type "clear"
Enter
Show

Type "datapeek --json samples/orders.csv | jq -c '.profile[] | {name, type, missing_pct, min, max}'"
Enter
Sleep 2.2s
