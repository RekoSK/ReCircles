# ReCircles

ReCircles is a native Rust launcher for Wayland. Run `recircles` to open a translucent ring around the pointer. Click a sector to open Files, Applications, Alt-Tab, Wi-Fi, or Bluetooth. [Visit website for more information.](https://recircles.rekosk.dev)

## Build and run

Install Rust 1.89 or later, Cargo, pkg-config, and the development files for GTK 3, gtk-layer-shell 0.6 or later, and libwayland-client. On Arch Linux, the native packages are `gtk3`, `gtk-layer-shell`, `wayland`, and `pkgconf`.

```sh
cargo install --locked --path .
recircles
```

Cargo installs to `~/.cargo/bin` by default. You can also run the project directly with `cargo run --release --locked`. To assign a shortcut, bind the installed command in your compositor.

## Documentation

Full docs which contain how to create your own themes, how to use the config file or how to create own menus are available [here](recircles.rekosk.dev/docs).

## License

ReCircles is licensed under the [GNU General Public License, version 3.0 only](LICENSE).

Copyright © 2026 Andrej "Reko" Revák.
