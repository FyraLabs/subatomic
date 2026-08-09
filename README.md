# subatomic & kiritan

> [!WARNING]
> This project is a work in progress.

<img align="left" style="vertical-align: middle" height="120" src="docs/kiritan-ssr.png" alt="Kiritan saying super special rapid">

`subatomic` is the modern package delivery system for RPMs.
`v1` of subatomic (`satm1`) is written in Rust, and comes with an extremely fast repodata generator,
around 10× faster and 30× more memory efficient than [`createrepo_c`] with cache.[^1]

- Repodata generation is also available via a separate binary `kiritan`.
- Interact with the API via the `satm` command.
- `satm`, `kiritan`, and the backend `libsubatomic` are inside the `crates/` directory.

> [!NOTE]
> `v0` of subatomic (`satm0`) was written in Go. You may find the source code in the `v0` branch.

## 🏗️ Building

Requires Rust nightly.

You should compile with `clang` to enable fat-LTO for `zstd`. See <https://lib.rs/crates/zstd-sys/features#feature-fat-lto>.

## 📃 License

    Copyright (C) 2026  Fyra Labs

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.


[^1]: See our blog for the benchmark: https://blog.fyralabs.com/kiritan-10x-faster-alternative-to-createrepo_c/

[`createrepo_c`]: https://github.com/rpm-software-management/createrepo_c/
