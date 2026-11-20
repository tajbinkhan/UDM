<!--
Copy this file to release-notes/vX.Y.Z.md before tagging vX.Y.Z. The
Release workflow uses it as the release description. Add the release to
CHANGELOG.md too, with its date and the summary sentence.

Release type, from the version number:
- major (1.0.0 -> 2.0.0): something that worked before stops working or
  changes how it is used, such as a removed option or a new settings format.
- minor (1.2.0 -> 1.3.0): new features, nothing breaks.
- patch (1.2.3 -> 1.2.4): bug fixes only.

Under Fixed, say what went wrong for the user, then what happens now, and
link the issue (#123) and commit (abc1234). Delete empty sections. Keep each
paragraph and list item on one line, because a release page shows every line
break.
-->
**Release type:** minor

One or two sentences on what this release is about.

## Breaking changes

- What changed, and what the user has to do about it.

## New

- A feature, said from the user's side.

## Changed

- Behavior that works differently now.

## Fixed

- What went wrong before, and what happens now. (#123, abc1234)

## Install

Pick the file for your system from the list below. The
[README](https://github.com/tajbinkhan/UDM#install) has the full steps.

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/). The [code signing policy](https://github.com/tajbinkhan/UDM#code-signing-policy) says who builds, reviews and approves each release.

The macOS disk image is experimental. It builds and passes CI on macOS, but it has not been run on a real Mac yet, so please [open an issue](https://github.com/tajbinkhan/UDM/issues) for anything that goes wrong.
