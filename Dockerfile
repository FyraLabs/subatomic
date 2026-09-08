FROM ghcr.io/terrapkg/builder:f44
ENV SERVER_HOST=localhost
ENV SERVER_PORT=3000
ENV DATABASE_URL=postgres://postgres:postgres@localhost:5432/subatomic
ENV STORAGE_DIR=./storage/
ENV CACHE_DIR=./cache/
ENV BODY_LIMIT=10737418240
ENV RUST_LOG=debug
RUN dnf in -y gcc 'pkgconfig(libzstd)' 'pkgconfig(openssl)' rustup
RUN rustup-init --default-toolchain nightly -y -q
RUN --mount=type=bind,source=.,target=. \
    PATH="$HOME/.cargo/bin:$PATH" cargo build --release
EXPOSE $SERVER_PORT
CMD ["./target/release/subatomic"]
