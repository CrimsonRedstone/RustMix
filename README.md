# RustMix Crimson Edition

A native mixer and patchbay for the MOTU 828 on Linux.

The window is a Rust desk for one unit on this computer. It mixes, names, and routes that 828, and it can also steer the programs playing on this PC. Until you connect, the desk is a demo and nothing is sent to the unit.

MOTU and the 828 name belong to MOTU. This project is not affiliated with them.

## The desk

The sidebar is the whole instrument. Connect and disconnect sit at the bottom of that list.

| Page | What you do there |
| --- | --- |
| Home | The front of the unit: what is playing, and the monitor and phones. |
| Device | The unit itself. Name, clock, sample rate, optical banks, and the firmware line. |
| Control | How RustMix opens on this PC. Login, the screen it sits on, locks, and the local assistant port. |
| Input | One analog channel at a time. Phase, EQ, and the compressor. |
| Output | One physical output at a time. Its name, its trim, and the meter of whatever is routed there. |
| Routing | The patchbay. A cable from a jack to a jack. Programs on this PC sit on the same page. |
| Hardware Mixer | The 828's own mixes. Level, pan, mute, and solo for each strip. |
| PC Mixer | Volume and mute for playback and recording on this computer, through PipeWire. These are not the 828's trims. |
| Discovery | The 828 on this computer. It does not scan the network. |

One process holds the ALSA port named `828 COM`. A second copy of RustMix, or another mixer using that port, has to wait. While the port is taken, this desk stays on the demo.

The 828 keeps the last mix that was written. Quitting does not put the old mix back.

Headphone, monitor, and output level are real hardware levels. The window sets them. The socket and the assistant port do not.

## Routing

Drag a cable from a source jack to an output jack. One source can feed many outputs. With Stereo on, the partner jack comes along. A path that would howl, or that would replace what a jack is already playing, asks first.

Playing programs are listed on the source side. Recording programs are listed on the destination side. A lock keeps that program on its route.

**Dummy** on Routing adds a host of your own. Apps and inputs that already reach the computer mix at **Dummy in**. **Dummy out** can feed one jack or several, including a single side when Stereo is off. Remove drops the host and puts those jacks back.

Default, Save, and Load are the patch memories.

## Requirements

Linux, a Wayland or X11 session, and Rust **1.85 or newer**. The crate uses edition 2024.

Arch Linux:

```sh
sudo pacman -S --needed base-devel alsa-lib alsa-utils pipewire pipewire-pulse libpulse noto-fonts mesa
```

Install Rust from [rustup](https://rustup.rs/) if `rustc --version` is older than 1.85 or missing. The `rust` package also provides `rustc` and `cargo`.

Other distributions need the same pieces:

- a C toolchain
- ALSA development files (`libasound`)
- PipeWire, with its Pulse layer
- `pactl`, `pw-dump`, `pw-metadata`, and `pw-link`
- `amidi`
- Noto Sans
- OpenGL (`libGL`)

The window reads Noto Sans from `/usr/share/fonts/noto/`. If those files are missing it still opens, on the built-in font. `qdbus6` (Arch: `qt6-tools`) is used only to pin the window to a screen under KDE. It is optional.

## Install

From this directory:

```sh
cargo build --release --features ui
install -Dm755 target/release/RustMix ~/.local/bin/RustMix
install -Dm644 assets/rmce.png ~/.local/share/icons/hicolor/256x256/apps/RustMix.png
```

A build without `--features ui` has no window. `~/.local/bin` needs to be on `PATH`.

Optional launcher. Save it as `~/.local/share/applications/RustMix.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=RustMix Crimson Edition
Exec=RustMix
Icon=RustMix
Terminal=false
Categories=AudioVideo;Audio;Mixer;
StartupWMClass=RustMix
```

Then, if the command exists:

```sh
update-desktop-database ~/.local/share/applications
```

## Run

```sh
RustMix
```

`RustMix --help` prints the short usage. A second launch, while the window is open, tells you it is already running and exits.

Settings are kept in `~/.config/RustMix/settings.json`, or `$XDG_CONFIG_HOME/RustMix/settings.json` when that variable is set. Launch at login is a switch on Control.

While the window is running it listens on `$XDG_RUNTIME_DIR/RustMix.sock` (mode 0600). `RustMix --mcp` speaks MCP on stdin to that window. It does not open the 828 itself. The TCP listener stays off until Control turns it on. The default address is `127.0.0.1` port `4771`.

To run the tests:

```sh
cargo test --features ui
```

## License

MIT OR Apache-2.0, at your option. Copyright 2026 Crimson.

The short statement is [LICENSE](LICENSE). The full texts are [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
