package schema

import (
	"entgo.io/ent"
	"entgo.io/ent/schema/field"
)

// SigningKey holds the schema definition for the SigningKey entity.
type SigningKey struct {
	ent.Schema
}

// Fields of the SigningKey.
func (SigningKey) Fields() []ent.Field {
	return []ent.Field{
		field.String("id").Unique().StorageKey("oid"),
		field.String("private_key"),
		field.String("public_key"),
		field.String("name"),
		field.String("email"),
	}
}

// Edges of the SigningKey.
func (SigningKey) Edges() []ent.Edge {
	return nil
}
