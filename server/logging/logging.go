package logging

import (
	"os"

	"github.com/go-kit/log"
	"github.com/go-kit/log/level"
)

func initLogger() log.Logger {
	logger := log.NewLogfmtLogger(os.Stdout)
	logger = level.NewFilter(logger, level.AllowAll())
	return logger
}

var Logger log.Logger = initLogger()
