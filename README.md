# Subatomic

A modern package delivery system for RPMs.

> [!WARNING]
> This README and docs are under progress.

## 🛠️ Dependencies

Please make sure you have these dependencies first before building.

```bash
golang
```

Additionally, you might want to install air, a live reload tool for Go apps. To install the latest version:

```bash
go install github.com/air-verse/air@latest
```

## 🏗️ Building

Simply clone this repo then:

```bash
go build ./(server|subatomic-cli)
```

## Client Configuration

`subatomic-cli` can be configured using a config file in `~/.config/subatomic.json`, or by using CLI flags.

```json
{
  "server": "https://subatomic.example.com",
  "token": "super-secret-jwt-token"
}
```

Currently, the JWT uses HS256. Here is an example payload:

```json
{
  "scopes": ["admin"]
}
```

## 🗒️ Todos

- Improve the README
- Refactor some bad (written while out of it) code
- ~~Deprecate OSTree~~ and go RPM only
