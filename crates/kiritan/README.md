# <ruby>きりたん<rp>(</rp><rt>`kiritan`</rt><rp>)</rp></ruby>

`kiritan` (quic**k** **i**nit **r**epo **i**ns**tan**tly…?) is the next-generation modern RPM
repository metadata generator. 

This requires Rust nightly to compile.

## 🔪 Performance

With a cache, `kiritan` is around 10×[^1] faster than `createrepo_c`.
`kiritan` also uses 30×[^2] less memory than `createrepo_c` regardless of caching.

You can view more information on our benchmarks on [this blog post].

## 📃 License

`AGPL-3.0-or-later`

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


[^1]: For around 5000 packages. `kiritan` only "gets faster" for even larger repositories.
[^2]: `kiritan` uses a constant amount of memory, i.e. the memory complexity is O(1).


[this blog post]: https://blog.fyralabs.com/p/06ec5fea-32de-416c-a7f5-0edbf79649da/?member_status=free
