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
go install github.com/cosmtrek/air@latest
```

## 🏗️ Building

Simply clone this repo then:

```bash
go build (server|subatomic-cli)
```

## 🗒️ Todos

- Improve the README
- Refactor some bad (written while out of it) code
- Move away from go work
- Deprecate OSTree and go RPM only
