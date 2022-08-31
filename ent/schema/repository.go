package schema

import (
	"entgo.io/ent"
	"entgo.io/ent/schema/field"
	"github.com/FyraLabs/subatomic/property"
)

// Repository holds the schema definition for the Repository entity.
type Repository struct {
	ent.Schema
}

// Fields of the Repository.
func (Repository) Fields() []ent.Field {
	return []ent.Field{
		field.String("name").Unique().StorageKey("oid"),
		field.Enum("type").GoType(property.RepositoryType(0)),
	}
}

// Edges of the Repository.
func (Repository) Edges() []ent.Edge {
	return nil
}
