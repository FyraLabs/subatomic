FROM golang:1.19-bullseye as builder

WORKDIR /app

RUN apt update && apt install -y build-essential libostree-dev

COPY . .

RUN go build -o /subatomic ./server

FROM golang:1.19-bullseye

COPY --from=builder /subatomic /subatomic

RUN apt update && apt install -y libostree-1-1 ostree createrepo-c

EXPOSE 3000

CMD [ "/subatomic" ]
