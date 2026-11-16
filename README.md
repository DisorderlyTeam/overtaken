# Overtaken

**Overtaken** is a tactical action stealth game in which the player, as
an experimental entity capable of taking control over machines, escapes from
a sealed research complexes aboard spaceships. Each robot has different
properties and limitations that determine how environmental puzzles are solved
and how threats are avoided. The player must strategically choose which targets
to overtake, bypass security systems, and use the environment to their advantage
to gradually uncover and escape route from the corporation-controlled facilities.

Received honorable mentions by [Game Access Conference][GAC] and [Młodzi w Łodzi]
at the [ZTGK (Team Video Game Development) competition][ZTGK] 2026.

You can download and play the latest stable build on [itch.io] or [GameJolt]!
That's also where you can support our team financially, if you want to.

## Licenses

The code in this repository including the `game` and `engine` crates as well as
the shaders in `assets/shaders` are licensed with [GNU AGPL 3.0 or later][AGPL-3.0].

Audio files (both music and sound effects) were taken from Pixabay under
the [Pixabay Content License].

The rest of the assets are licensed with [CC BY-NC-SA 4.0].

## Development

The preferred method to build, run, or export this project is using [just](https://github.com/casey/just).

### Without `just`

This project requires patching gil-rs for fixing gamepad rumble on Linux.
It's automated with `just`, so if you don't want to use it, you'll need to run
`cargo run -q --manifest-path vendor_tool/Cargo.toml -- ensure-vendor` yourself,
from the root directory of the project.

Then, you can simply use `cargo` to build or run the project (by default, the game). It shouldn't require any external
dependencies. To export, you can run the `exporter` package.

You can also see the [justfile](justfile) manually to see the commands to use.

[itch.io]: https://disorderlyteam.itch.io/overtaken
[GameJolt]: https://gamejolt.com/games/overtaken/1081361

[GAC]: https://game-access.com/conference/
[Młodzi w Łodzi]: https://mlodziwlodzi.pl/
[ZTGK]: https://ztgk.pl/

[AGPL-3.0]: https://www.gnu.org/licenses/agpl-3.0.en.html
[CC BY-NC-SA 4.0]: https://creativecommons.org/licenses/by-nc-sa/4.0/
[Pixabay Content License]: https://pixabay.com/service/license-summary/
