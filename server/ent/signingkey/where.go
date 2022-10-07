// Code generated by ent, DO NOT EDIT.

package signingkey

import (
	"entgo.io/ent/dialect/sql"
	"entgo.io/ent/dialect/sql/sqlgraph"
	"github.com/FyraLabs/subatomic/server/ent/predicate"
)

// ID filters vertices based on their ID field.
func ID(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldID), id))
	})
}

// IDEQ applies the EQ predicate on the ID field.
func IDEQ(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldID), id))
	})
}

// IDNEQ applies the NEQ predicate on the ID field.
func IDNEQ(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NEQ(s.C(FieldID), id))
	})
}

// IDIn applies the In predicate on the ID field.
func IDIn(ids ...string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		v := make([]interface{}, len(ids))
		for i := range v {
			v[i] = ids[i]
		}
		s.Where(sql.In(s.C(FieldID), v...))
	})
}

// IDNotIn applies the NotIn predicate on the ID field.
func IDNotIn(ids ...string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		v := make([]interface{}, len(ids))
		for i := range v {
			v[i] = ids[i]
		}
		s.Where(sql.NotIn(s.C(FieldID), v...))
	})
}

// IDGT applies the GT predicate on the ID field.
func IDGT(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GT(s.C(FieldID), id))
	})
}

// IDGTE applies the GTE predicate on the ID field.
func IDGTE(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GTE(s.C(FieldID), id))
	})
}

// IDLT applies the LT predicate on the ID field.
func IDLT(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LT(s.C(FieldID), id))
	})
}

// IDLTE applies the LTE predicate on the ID field.
func IDLTE(id string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LTE(s.C(FieldID), id))
	})
}

// PrivateKey applies equality check predicate on the "private_key" field. It's identical to PrivateKeyEQ.
func PrivateKey(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldPrivateKey), v))
	})
}

// PublicKey applies equality check predicate on the "public_key" field. It's identical to PublicKeyEQ.
func PublicKey(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldPublicKey), v))
	})
}

// Name applies equality check predicate on the "name" field. It's identical to NameEQ.
func Name(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldName), v))
	})
}

// Email applies equality check predicate on the "email" field. It's identical to EmailEQ.
func Email(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldEmail), v))
	})
}

// PrivateKeyEQ applies the EQ predicate on the "private_key" field.
func PrivateKeyEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyNEQ applies the NEQ predicate on the "private_key" field.
func PrivateKeyNEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NEQ(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyIn applies the In predicate on the "private_key" field.
func PrivateKeyIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.In(s.C(FieldPrivateKey), v...))
	})
}

// PrivateKeyNotIn applies the NotIn predicate on the "private_key" field.
func PrivateKeyNotIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NotIn(s.C(FieldPrivateKey), v...))
	})
}

// PrivateKeyGT applies the GT predicate on the "private_key" field.
func PrivateKeyGT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GT(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyGTE applies the GTE predicate on the "private_key" field.
func PrivateKeyGTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GTE(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyLT applies the LT predicate on the "private_key" field.
func PrivateKeyLT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LT(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyLTE applies the LTE predicate on the "private_key" field.
func PrivateKeyLTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LTE(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyContains applies the Contains predicate on the "private_key" field.
func PrivateKeyContains(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.Contains(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyHasPrefix applies the HasPrefix predicate on the "private_key" field.
func PrivateKeyHasPrefix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasPrefix(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyHasSuffix applies the HasSuffix predicate on the "private_key" field.
func PrivateKeyHasSuffix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasSuffix(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyEqualFold applies the EqualFold predicate on the "private_key" field.
func PrivateKeyEqualFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EqualFold(s.C(FieldPrivateKey), v))
	})
}

// PrivateKeyContainsFold applies the ContainsFold predicate on the "private_key" field.
func PrivateKeyContainsFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.ContainsFold(s.C(FieldPrivateKey), v))
	})
}

// PublicKeyEQ applies the EQ predicate on the "public_key" field.
func PublicKeyEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldPublicKey), v))
	})
}

// PublicKeyNEQ applies the NEQ predicate on the "public_key" field.
func PublicKeyNEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NEQ(s.C(FieldPublicKey), v))
	})
}

// PublicKeyIn applies the In predicate on the "public_key" field.
func PublicKeyIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.In(s.C(FieldPublicKey), v...))
	})
}

// PublicKeyNotIn applies the NotIn predicate on the "public_key" field.
func PublicKeyNotIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NotIn(s.C(FieldPublicKey), v...))
	})
}

// PublicKeyGT applies the GT predicate on the "public_key" field.
func PublicKeyGT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GT(s.C(FieldPublicKey), v))
	})
}

// PublicKeyGTE applies the GTE predicate on the "public_key" field.
func PublicKeyGTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GTE(s.C(FieldPublicKey), v))
	})
}

// PublicKeyLT applies the LT predicate on the "public_key" field.
func PublicKeyLT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LT(s.C(FieldPublicKey), v))
	})
}

// PublicKeyLTE applies the LTE predicate on the "public_key" field.
func PublicKeyLTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LTE(s.C(FieldPublicKey), v))
	})
}

// PublicKeyContains applies the Contains predicate on the "public_key" field.
func PublicKeyContains(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.Contains(s.C(FieldPublicKey), v))
	})
}

// PublicKeyHasPrefix applies the HasPrefix predicate on the "public_key" field.
func PublicKeyHasPrefix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasPrefix(s.C(FieldPublicKey), v))
	})
}

// PublicKeyHasSuffix applies the HasSuffix predicate on the "public_key" field.
func PublicKeyHasSuffix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasSuffix(s.C(FieldPublicKey), v))
	})
}

// PublicKeyEqualFold applies the EqualFold predicate on the "public_key" field.
func PublicKeyEqualFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EqualFold(s.C(FieldPublicKey), v))
	})
}

// PublicKeyContainsFold applies the ContainsFold predicate on the "public_key" field.
func PublicKeyContainsFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.ContainsFold(s.C(FieldPublicKey), v))
	})
}

// NameEQ applies the EQ predicate on the "name" field.
func NameEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldName), v))
	})
}

// NameNEQ applies the NEQ predicate on the "name" field.
func NameNEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NEQ(s.C(FieldName), v))
	})
}

// NameIn applies the In predicate on the "name" field.
func NameIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.In(s.C(FieldName), v...))
	})
}

// NameNotIn applies the NotIn predicate on the "name" field.
func NameNotIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NotIn(s.C(FieldName), v...))
	})
}

// NameGT applies the GT predicate on the "name" field.
func NameGT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GT(s.C(FieldName), v))
	})
}

// NameGTE applies the GTE predicate on the "name" field.
func NameGTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GTE(s.C(FieldName), v))
	})
}

// NameLT applies the LT predicate on the "name" field.
func NameLT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LT(s.C(FieldName), v))
	})
}

// NameLTE applies the LTE predicate on the "name" field.
func NameLTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LTE(s.C(FieldName), v))
	})
}

// NameContains applies the Contains predicate on the "name" field.
func NameContains(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.Contains(s.C(FieldName), v))
	})
}

// NameHasPrefix applies the HasPrefix predicate on the "name" field.
func NameHasPrefix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasPrefix(s.C(FieldName), v))
	})
}

// NameHasSuffix applies the HasSuffix predicate on the "name" field.
func NameHasSuffix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasSuffix(s.C(FieldName), v))
	})
}

// NameEqualFold applies the EqualFold predicate on the "name" field.
func NameEqualFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EqualFold(s.C(FieldName), v))
	})
}

// NameContainsFold applies the ContainsFold predicate on the "name" field.
func NameContainsFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.ContainsFold(s.C(FieldName), v))
	})
}

// EmailEQ applies the EQ predicate on the "email" field.
func EmailEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EQ(s.C(FieldEmail), v))
	})
}

// EmailNEQ applies the NEQ predicate on the "email" field.
func EmailNEQ(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NEQ(s.C(FieldEmail), v))
	})
}

// EmailIn applies the In predicate on the "email" field.
func EmailIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.In(s.C(FieldEmail), v...))
	})
}

// EmailNotIn applies the NotIn predicate on the "email" field.
func EmailNotIn(vs ...string) predicate.SigningKey {
	v := make([]interface{}, len(vs))
	for i := range v {
		v[i] = vs[i]
	}
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.NotIn(s.C(FieldEmail), v...))
	})
}

// EmailGT applies the GT predicate on the "email" field.
func EmailGT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GT(s.C(FieldEmail), v))
	})
}

// EmailGTE applies the GTE predicate on the "email" field.
func EmailGTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.GTE(s.C(FieldEmail), v))
	})
}

// EmailLT applies the LT predicate on the "email" field.
func EmailLT(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LT(s.C(FieldEmail), v))
	})
}

// EmailLTE applies the LTE predicate on the "email" field.
func EmailLTE(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.LTE(s.C(FieldEmail), v))
	})
}

// EmailContains applies the Contains predicate on the "email" field.
func EmailContains(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.Contains(s.C(FieldEmail), v))
	})
}

// EmailHasPrefix applies the HasPrefix predicate on the "email" field.
func EmailHasPrefix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasPrefix(s.C(FieldEmail), v))
	})
}

// EmailHasSuffix applies the HasSuffix predicate on the "email" field.
func EmailHasSuffix(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.HasSuffix(s.C(FieldEmail), v))
	})
}

// EmailEqualFold applies the EqualFold predicate on the "email" field.
func EmailEqualFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.EqualFold(s.C(FieldEmail), v))
	})
}

// EmailContainsFold applies the ContainsFold predicate on the "email" field.
func EmailContainsFold(v string) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s.Where(sql.ContainsFold(s.C(FieldEmail), v))
	})
}

// HasRepo applies the HasEdge predicate on the "repo" edge.
func HasRepo() predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		step := sqlgraph.NewStep(
			sqlgraph.From(Table, FieldID),
			sqlgraph.To(RepoTable, FieldID),
			sqlgraph.Edge(sqlgraph.O2M, true, RepoTable, RepoColumn),
		)
		sqlgraph.HasNeighbors(s, step)
	})
}

// HasRepoWith applies the HasEdge predicate on the "repo" edge with a given conditions (other predicates).
func HasRepoWith(preds ...predicate.Repo) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		step := sqlgraph.NewStep(
			sqlgraph.From(Table, FieldID),
			sqlgraph.To(RepoInverseTable, FieldID),
			sqlgraph.Edge(sqlgraph.O2M, true, RepoTable, RepoColumn),
		)
		sqlgraph.HasNeighborsWith(s, step, func(s *sql.Selector) {
			for _, p := range preds {
				p(s)
			}
		})
	})
}

// And groups predicates with the AND operator between them.
func And(predicates ...predicate.SigningKey) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s1 := s.Clone().SetP(nil)
		for _, p := range predicates {
			p(s1)
		}
		s.Where(s1.P())
	})
}

// Or groups predicates with the OR operator between them.
func Or(predicates ...predicate.SigningKey) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		s1 := s.Clone().SetP(nil)
		for i, p := range predicates {
			if i > 0 {
				s1.Or()
			}
			p(s1)
		}
		s.Where(s1.P())
	})
}

// Not applies the not operator on the given predicate.
func Not(p predicate.SigningKey) predicate.SigningKey {
	return predicate.SigningKey(func(s *sql.Selector) {
		p(s.Not())
	})
}