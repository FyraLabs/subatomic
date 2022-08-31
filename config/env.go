package config

import (
	env "github.com/Netflix/go-env"
	"github.com/joho/godotenv"
)

type EnviromentType struct {
	StorageDirectory string `env:"STORAGE_DIRECTORY,required=true"`
	DatabaseOptions  string `env:"DATABASE_OPTIONS,required=true"`
}

var Environment EnviromentType

func InitializeEnv() error {
	_ = godotenv.Load()
	_, err := env.UnmarshalFromEnviron(&Environment)
	if err != nil {
		return err
	}

	return nil
}
