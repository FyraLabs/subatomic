FROM golang:1.21-bookworm as builder

WORKDIR /app

RUN apt update && apt install -y build-essential

COPY . .

RUN go build -o /subatomic ./server

FROM golang:1.21-bookworm

COPY --from=builder /subatomic /subatomic

RUN apt update && apt install -y createrepo-c

EXPOSE 3000

CMD [ "/subatomic" ]
