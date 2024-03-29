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
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/propagation"

	_ "github.com/FyraLabs/subatomic/server/docs"
	_ "github.com/lib/pq"
	_ "github.com/swaggo/files"
)

var validate *validator.Validate
var decoder *form.Decoder

//	@title			Subatomic
//	@version		1.0
//	@description	A modern package delivery server.
//	@BasePath		/

//	@license.name	GPL3
//	@license.url	https://choosealicense.com/licenses/gpl-3.0/

// @securityDefinitions.apikey
// @in		header
// @name	Authorization
func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() error {
	validate = validator.New()
	decoder = form.NewDecoder()
	var environment types.Environment

	_ = godotenv.Load()
	_, err := env.UnmarshalFromEnviron(&environment)
	if err != nil {
		return err
	}

	client, err := ent.Open("postgres", environment.DatabaseOptions)
	if err != nil {
		return err
	}

	if err := client.Schema.Create(context.Background()); err != nil {
		return fmt.Errorf("failed creating schema resources: %w", err)
	}

	if environment.EnableTracing {
		tp := initTracerProvider()
		defer func() {
			if err := tp.Shutdown(context.Background()); err != nil {
				log.Printf("Error shutting down tracer provider: %v", err)
			}
		}()

		otel.SetTracerProvider(tp)
		otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(propagation.TraceContext{}, propagation.Baggage{}))
	}

	// TODO: Auth
	router := &apiRouter{
		database:         client,
		environment:      &environment,
		jwtAuthenticator: jwtauth.New("HS256", []byte(environment.JWTSecret), nil),
		repoMutex:        &keyedmutex.KeyedMutex{},
	}
	router.setup()

	println("Listening on :3000")
	return http.ListenAndServe(":3000", router)
}
