package schema

import (
	"entgo.io/ent"
	"entgo.io/ent/schema/edge"
	"entgo.io/ent/schema/field"
	"entgo.io/ent/schema/index"
)

// RpmPackage holds the schema definition for the RpmPackage entity.
type RpmPackage struct {
	ent.Schema
}

// Fields of the RpmPackage.
func (RpmPackage) Fields() []ent.Field {
	return []ent.Field{
		field.String("name"),
		field.Int("epoch").Min(0),
		field.String("version"),
		field.String("release"),
		field.String("arch"),
		field.String("file_path"),
	}
}

// Edges of the RpmPackage.
func (RpmPackage) Edges() []ent.Edge {
	return []ent.Edge{
		edge.From("repo", Repo.Type).
			Ref("rpms").
			Unique(),
	}
}

// Indexes of the RpmPackage.
func (RpmPackage) Indexes() []ent.Index {
	return []ent.Index{
		index.Fields("file_path").
			Edges("repo").
			Unique(),
		index.Fields("name", "epoch", "version", "release", "arch").
			Unique(),
	}
}
