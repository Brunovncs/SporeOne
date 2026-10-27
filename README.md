# SporeOne

<img src="assets/icon-256.png" width="96" align="right" alt="">

A mod manager for Spore on Windows. It lists the community's mods in one catalog, installs any of
them in one click with whatever else it needs, keeps them up to date, and starts the game with
them. It does not replace the Spore ModAPI Launcher Kit: it installs mods into it, the same way its
Easy Installer does, so the Launcher Kit's Easy Uninstaller shows the same mods and either program
can remove what the other installed.

SporeOne is open source (MIT) and new, at version 0.1.0. It is not made by EA or Maxis, and it does
not check the mods it lists: they are their authors' work.

![SporeOne's catalog: mods as tiles with their category, author, downloads and version, one marked Installed and one with an update](docs/catalog.png)

## What you need

Windows 10 or 11, Spore with Galactic Adventures from Steam, GOG, the EA app or the disc, and the
Launcher Kit. If the Launcher Kit is missing, SporeOne downloads its setup from its GitHub release
and starts it. A Steam copy has to be started once from Steam before mods can go in, so that it
tells Windows where it is.

## Where the mods come from

Most current Spore mods are published on GitHub, in a repository tagged `spore-mod` with a
`ModInfo.xml` and its `.sporemod` or `.package` attached to a release. That is what the
[Spore Mod Browser](https://mods.sporecommunity.com) lists, and SporeOne reads the same
repositories. A workflow in this repository builds the catalog from them every six hours, downloads
each file once to record its SHA-256, and publishes it on the `catalog` branch; SporeOne downloads
that file when it opens, and carries a copy from its own release for when it is offline. A
download that does not match its hash is not installed.

`catalog/curated.json`, kept by hand, adds what GitHub cannot say: older mods published on other
sites, mods that come with their own installer, which store's copy a mod needs, and notes. Mods
from Nexus Mods, ModDB or Davo's forum are listed with a link to their page, since those sites need
an account or a browser to download; the file downloaded there can be dropped on SporeOne's window
to install it.

To get a mod into the catalog, publish it the way the Mod Browser asks (the `spore-mod` topic, a
`ModInfo.xml` at the root of the repository, the file on a release) and it shows up on the next
build. Corrections to the curated list are welcome as pull requests.

## Before installing a mod

A mod's page says whether it will work on this computer, before anything is downloaded:

- the game: whether Galactic Adventures is installed, and whether the build of `SporeApp.exe` is one
  the Launcher Kit supports (disc with patch 5.1, March 2017 or October 2024 from Steam, GOG or the
  EA app), told apart by its size as the Launcher Kit does;
- the Launcher Kit, and whether its ModAPI DLLs are as new as the mod's `dllsBuild`;
- the mods it depends on (`depends` in `ModInfo.xml`), which are installed with it when the catalog
  has them, in order;
- what its author flags: experimental, needs a galaxy reset, saved games depend on it;
- installed mods whose files it would replace, and mods it is known not to work with;
- SporeModLoader, which the Launcher Kit will not start the game next to.

Mods with options (a custom installer in `ModInfo.xml`) show them on their page, and the choices
are kept where the Launcher Kit keeps them. Uninstalling a mod that others need says so first.

## Multiplayer

[SporeCoop](https://github.com/WoodyWizard/SporeCoop) is a co-op mod for two to four players that
aims to cover every stage. It is new and experimental. It has an installer of its own and does not go through the Launcher
Kit, so SporeOne downloads that installer and starts it, and tells from the game's folder whether
it is in. It works with the Steam and GOG copies only.

## Installing into the game's folder

The game is usually in Program Files. Each install is worked out as a plan first: the files to copy
and delete, and the Launcher Kit's list written last. When the folders need administrator rights,
SporeOne starts itself again with them only to carry that plan out, after checking that every path
is inside the game's data folders or the Launcher Kit's folder. No plan can delete or replace a file
named like the game's own (`Spore_*.package`, `EP1_*` and the like), even when a mod's
`ModInfo.xml` asks for it.

## Building

SporeOne is written in Rust with [GPUI](https://www.gpui.rs), and looks like
[OpenController](https://github.com/Brunovncs/OpenController).

```
cargo build --release
cargo test --workspace
cargo run -- --demo
```

`--demo` shows a made-up game and Launcher Kit in a temporary folder, with a few mods in, so the
window can be tried without Spore; installing from it is real, into that folder.
`cargo test -p sporeone-core --test real_mods -- --ignored` downloads popular mods from the catalog
and installs and removes them there. `sporeone-catalog` builds the catalog
(`GITHUB_TOKEN` or `GH_TOKEN` set, since GitHub allows few requests without one).

There are no release builds yet.

## License

MIT. Spore is a trademark of Electronic Arts.
