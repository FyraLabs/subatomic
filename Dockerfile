FROM ghcr.io/terrapkg/builder:f44 AS terra
WORKDIR /subatomic
RUN dnf in -y gcc 'pkgconfig(libzstd)' 'pkgconfig(openssl)' rustup /usr/lib/rpm/rpmdeps
RUN rustup-init --default-toolchain nightly -y -q
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
COPY ./crates     ./crates
COPY ./src        ./src/
COPY ./migrations ./migrations
COPY ./.sqlx      ./.sqlx
RUN PATH="$HOME/.cargo/bin:$PATH" ZSTD_SYS_USE_PKG_CONFIG=1 cargo build --release
RUN /usr/lib/rpm/rpmdeps --define="_use_internal_dependency_generator 1" --requires target/release/subatomic > deps.txt

FROM registry.fedoraproject.org/fedora-minimal:44
COPY --from=terra /subatomic/target/release/subatomic .
COPY --from=terra /subatomic/deps.txt .
# install dynamically linked libraries
RUN dnf in -y --setopt=install_weak_deps=0 $(cat deps.txt)
ENV DATABASE_URL=postgres://postgres:postgres@localhost:5432/subatomic
ENV STORAGE_DIR=./storage/
ENV CACHE_DIR=./cache/
ENV BODY_LIMIT=10737418240
ENV RUST_LOG=debug
EXPOSE $SERVER_PORT
CMD ["./subatomic"]
