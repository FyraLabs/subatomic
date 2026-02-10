FROM golang:1.25.2-trixie as builder

WORKDIR /app

COPY go.mod go.sum ./

RUN --mount=type=cache,target=/go/pkg/mod \
    go mod download

COPY . .

RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    go build -o /subatomic ./server

FROM debian:trixie

COPY --from=builder /subatomic /subatomic

RUN apt update && apt install -y createrepo-c

EXPOSE 3000

CMD [ "/subatomic" ]
