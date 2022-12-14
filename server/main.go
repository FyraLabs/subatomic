package main

import (
	"context"
	"fmt"
	"log"
	"net/http"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/keyedmutex"
	"github.com/FyraLabs/subatomic/server/types"
	"github.com/Netflix/go-env"
	"github.com/go-chi/jwtauth/v5"
	"github.com/go-playground/form/v4"
	"github.com/go-playground/validator/v10"
	"github.com/joho/godotenv"

	_ "github.com/FyraLabs/subatomic/server/docs"
	_ "github.com/lib/pq"
	_ "github.com/swaggo/files"
)

var validate *validator.Validate
var decoder *form.Decoder

// @title       Subatomic
// @version     1.0
// @description A modern package delivery server.
// @BasePath    /

// @license.name GPL3
// @license.url  https://choosealicense.com/licenses/gpl-3.0/

// @securityDefinitions.apikey
// @in   header
// @name Authorization
func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() error {
	validate = validator.New()
	decoder = form.NewDecoder()
	var enviroment types.Enviroment

	_ = godotenv.Load()
	_, err := env.UnmarshalFromEnviron(&enviroment)
	if err != nil {
		return err
	}

	client, err := ent.Open("postgres", enviroment.DatabaseOptions)
	if err != nil {
		return err
	}

	if err := client.Schema.Create(context.Background()); err != nil {
		return fmt.Errorf("failed creating schema resources: %w", err)
	}

	// TODO: Auth
	router := &apiRouter{
		database:         client,
		enviroment:       &enviroment,
		jwtAuthenticator: jwtauth.New("HS256", []byte(enviroment.JWTSecret), nil),
		repoMutex:        &keyedmutex.KeyedMutex{},
	}
	router.setup()

	println("Listening on :3000")
	return http.ListenAndServe(":3000", router)
}
