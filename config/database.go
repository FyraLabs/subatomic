package config

import (
	"context"
	"log"

	"github.com/FyraLabs/subatomic/ent"
	_ "github.com/lib/pq"
)

var DatabaseClient *ent.Client

func InitializeDatabase() error {
	client, err := ent.Open("postgres", Environment.DatabaseOptions)
	if err != nil {
		return err
	}

	if err := client.Schema.Create(context.Background()); err != nil {
		log.Fatalf("failed creating schema resources: %v", err)
	}

	DatabaseClient = client

	return nil
}
