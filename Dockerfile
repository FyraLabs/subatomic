FROM golang:1.19-alpine as builder

WORKDIR /app

RUN apk add --no-cache gcc pkgconfig ostree-dev musl-dev

COPY server/go.mod ./
COPY server/go.sum ./
RUN go mod download

COPY server/*.go ./
COPY server/ent ./ent
COPY server/docs ./docs

RUN go build -o /subatomic

FROM golang:1.19-alpine

COPY --from=builder /subatomic /subatomic

RUN apk add --no-cache ostree

EXPOSE 3000

CMD [ "/subatomic" ]
