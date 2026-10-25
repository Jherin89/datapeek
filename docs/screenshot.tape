# Still of datapeek on the Excel sample (first sheet).
# From the repo root, with ttyd, ffmpeg, and a release build:
#   cargo build --release
#   export PATH="$PWD/target/release:$PATH"
#   vhs docs/screenshot.tape
# Writes docs/screenshot.png. docs/shot-preview.gif is a vhs requirement; delete it.

Output docs/shot-preview.gif

Set Shell bash
Set FontFamily "DejaVu Sans Mono"
Set FontSize 15
Set Width 1280
Set Height 680
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 8ms
Set Framerate 8
Set PlaybackSpeed 1
Set WindowBar Colorful
Set CursorBlink false

Hide
Type "export PS1='$ '"
Enter
Type "clear"
Enter
Show

Type "datapeek samples/orders.xlsx"
Enter
Sleep 1.2s
Screenshot docs/screenshot.png
Sleep 500ms
