package schema

import (
	"entgo.io/ent"
	"entgo.io/ent/schema/edge"
	"entgo.io/ent/schema/field"
)

// Repo holds the schema definition for the Repo entity.
type Repo struct {
	ent.Schema
}

// Fields of the Repo.
func (Repo) Fields() []ent.Field {
	return []ent.Field{
		field.String("id").Unique().StorageKey("oid"),
		field.Enum("type").Values("rpm", "ostree"),
	}
}

// Edges of the Repo.
func (Repo) Edges() []ent.Edge {
	return []ent.Edge{
		edge.To("rpms", RpmPackage.Type),
		edge.To("key", SigningKey.Type).Unique(),
	}
}
