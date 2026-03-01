# Demo recording for sf-pkgen README
# Prerequisites:
#   - sf CLI v2 with an authenticated Salesforce org (alias: dev)
#   - sf-pkgen installed
#
# Usage: vhs docs/demo.tape

Output docs/demo.gif

Require sf-pkgen

Set Shell "zsh"
Set FontSize 16
Set Width 1200
Set Height 600
Set Padding 10
Set TypingSpeed 75ms
Set Theme "Catppuccin Mocha"
Set WindowBar "Colorful"

# Clean up previous output (hidden from recording)
Hide
Type "rm -f package.xml"
Enter
Sleep 500ms
Ctrl+L
Show

# Phase 1: Launch
Type "sf-pkgen generate -o dev -a 66.0 -f package.xml"
Sleep 500ms
Enter
Sleep 2s

# Phase 2: Browse metadata types
Sleep 3s
Down 3
Sleep 400ms
Down 2
Sleep 2s

# Phase 3: Fuzzy search for ApexClass
Type "/"
Sleep 300ms
Type@150ms "apex"
Sleep 1s
Enter
Sleep 1s

# Phase 4: Select individual ApexClass components
Tab
Sleep 500ms
# Skip wildcard (*), move to individual components
Down
Sleep 300ms
Space
Sleep 500ms
Down
Sleep 300ms
Space
Sleep 800ms

# Phase 5: Wildcard selection for GenAiPlannerBundle
Tab
Sleep 300ms
Type "/"
Sleep 300ms
Type@150ms "genaiplanner"
Sleep 800ms
Enter
Sleep 1s
Tab
Sleep 500ms
Space
Sleep 800ms

# Phase 6: Confirm and show result
Enter
Sleep 2s
Type "cat package.xml"
Enter
Sleep 3s
