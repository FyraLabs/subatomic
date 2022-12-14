// Code generated by ent, DO NOT EDIT.

package ent

import (
	"context"
	"errors"
	"fmt"

	"entgo.io/ent/dialect/sql/sqlgraph"
	"entgo.io/ent/schema/field"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/signingkey"
)

// SigningKeyCreate is the builder for creating a SigningKey entity.
type SigningKeyCreate struct {
	config
	mutation *SigningKeyMutation
	hooks    []Hook
}

// SetPrivateKey sets the "private_key" field.
func (skc *SigningKeyCreate) SetPrivateKey(s string) *SigningKeyCreate {
	skc.mutation.SetPrivateKey(s)
	return skc
}

// SetPublicKey sets the "public_key" field.
func (skc *SigningKeyCreate) SetPublicKey(s string) *SigningKeyCreate {
	skc.mutation.SetPublicKey(s)
	return skc
}

// SetName sets the "name" field.
func (skc *SigningKeyCreate) SetName(s string) *SigningKeyCreate {
	skc.mutation.SetName(s)
	return skc
}

// SetEmail sets the "email" field.
func (skc *SigningKeyCreate) SetEmail(s string) *SigningKeyCreate {
	skc.mutation.SetEmail(s)
	return skc
}

// SetID sets the "id" field.
func (skc *SigningKeyCreate) SetID(s string) *SigningKeyCreate {
	skc.mutation.SetID(s)
	return skc
}

// AddRepoIDs adds the "repo" edge to the Repo entity by IDs.
func (skc *SigningKeyCreate) AddRepoIDs(ids ...string) *SigningKeyCreate {
	skc.mutation.AddRepoIDs(ids...)
	return skc
}

// AddRepo adds the "repo" edges to the Repo entity.
func (skc *SigningKeyCreate) AddRepo(r ...*Repo) *SigningKeyCreate {
	ids := make([]string, len(r))
	for i := range r {
		ids[i] = r[i].ID
	}
	return skc.AddRepoIDs(ids...)
}

// Mutation returns the SigningKeyMutation object of the builder.
func (skc *SigningKeyCreate) Mutation() *SigningKeyMutation {
	return skc.mutation
}

// Save creates the SigningKey in the database.
func (skc *SigningKeyCreate) Save(ctx context.Context) (*SigningKey, error) {
	var (
		err  error
		node *SigningKey
	)
	if len(skc.hooks) == 0 {
		if err = skc.check(); err != nil {
			return nil, err
		}
		node, err = skc.sqlSave(ctx)
	} else {
		var mut Mutator = MutateFunc(func(ctx context.Context, m Mutation) (Value, error) {
			mutation, ok := m.(*SigningKeyMutation)
			if !ok {
				return nil, fmt.Errorf("unexpected mutation type %T", m)
			}
			if err = skc.check(); err != nil {
				return nil, err
			}
			skc.mutation = mutation
			if node, err = skc.sqlSave(ctx); err != nil {
				return nil, err
			}
			mutation.id = &node.ID
			mutation.done = true
			return node, err
		})
		for i := len(skc.hooks) - 1; i >= 0; i-- {
			if skc.hooks[i] == nil {
				return nil, fmt.Errorf("ent: uninitialized hook (forgotten import ent/runtime?)")
			}
			mut = skc.hooks[i](mut)
		}
		v, err := mut.Mutate(ctx, skc.mutation)
		if err != nil {
			return nil, err
		}
		nv, ok := v.(*SigningKey)
		if !ok {
			return nil, fmt.Errorf("unexpected node type %T returned from SigningKeyMutation", v)
		}
		node = nv
	}
	return node, err
}

// SaveX calls Save and panics if Save returns an error.
func (skc *SigningKeyCreate) SaveX(ctx context.Context) *SigningKey {
	v, err := skc.Save(ctx)
	if err != nil {
		panic(err)
	}
	return v
}

// Exec executes the query.
func (skc *SigningKeyCreate) Exec(ctx context.Context) error {
	_, err := skc.Save(ctx)
	return err
}

// ExecX is like Exec, but panics if an error occurs.
func (skc *SigningKeyCreate) ExecX(ctx context.Context) {
	if err := skc.Exec(ctx); err != nil {
		panic(err)
	}
}

// check runs all checks and user-defined validators on the builder.
func (skc *SigningKeyCreate) check() error {
	if _, ok := skc.mutation.PrivateKey(); !ok {
		return &ValidationError{Name: "private_key", err: errors.New(`ent: missing required field "SigningKey.private_key"`)}
	}
	if _, ok := skc.mutation.PublicKey(); !ok {
		return &ValidationError{Name: "public_key", err: errors.New(`ent: missing required field "SigningKey.public_key"`)}
	}
	if _, ok := skc.mutation.Name(); !ok {
		return &ValidationError{Name: "name", err: errors.New(`ent: missing required field "SigningKey.name"`)}
	}
	if _, ok := skc.mutation.Email(); !ok {
		return &ValidationError{Name: "email", err: errors.New(`ent: missing required field "SigningKey.email"`)}
	}
	return nil
}

func (skc *SigningKeyCreate) sqlSave(ctx context.Context) (*SigningKey, error) {
	_node, _spec := skc.createSpec()
	if err := sqlgraph.CreateNode(ctx, skc.driver, _spec); err != nil {
		if sqlgraph.IsConstraintError(err) {
			err = &ConstraintError{msg: err.Error(), wrap: err}
		}
		return nil, err
	}
	if _spec.ID.Value != nil {
		if id, ok := _spec.ID.Value.(string); ok {
			_node.ID = id
		} else {
			return nil, fmt.Errorf("unexpected SigningKey.ID type: %T", _spec.ID.Value)
		}
	}
	return _node, nil
}

func (skc *SigningKeyCreate) createSpec() (*SigningKey, *sqlgraph.CreateSpec) {
	var (
		_node = &SigningKey{config: skc.config}
		_spec = &sqlgraph.CreateSpec{
			Table: signingkey.Table,
			ID: &sqlgraph.FieldSpec{
				Type:   field.TypeString,
				Column: signingkey.FieldID,
			},
		}
	)
	if id, ok := skc.mutation.ID(); ok {
		_node.ID = id
		_spec.ID.Value = id
	}
	if value, ok := skc.mutation.PrivateKey(); ok {
		_spec.Fields = append(_spec.Fields, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: signingkey.FieldPrivateKey,
		})
		_node.PrivateKey = value
	}
	if value, ok := skc.mutation.PublicKey(); ok {
		_spec.Fields = append(_spec.Fields, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: signingkey.FieldPublicKey,
		})
		_node.PublicKey = value
	}
	if value, ok := skc.mutation.Name(); ok {
		_spec.Fields = append(_spec.Fields, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: signingkey.FieldName,
		})
		_node.Name = value
	}
	if value, ok := skc.mutation.Email(); ok {
		_spec.Fields = append(_spec.Fields, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: signingkey.FieldEmail,
		})
		_node.Email = value
	}
	if nodes := skc.mutation.RepoIDs(); len(nodes) > 0 {
		edge := &sqlgraph.EdgeSpec{
			Rel:     sqlgraph.O2M,
			Inverse: true,
			Table:   signingkey.RepoTable,
			Columns: []string{signingkey.RepoColumn},
			Bidi:    false,
			Target: &sqlgraph.EdgeTarget{
				IDSpec: &sqlgraph.FieldSpec{
					Type:   field.TypeString,
					Column: repo.FieldID,
				},
			},
		}
		for _, k := range nodes {
			edge.Target.Nodes = append(edge.Target.Nodes, k)
		}
		_spec.Edges = append(_spec.Edges, edge)
	}
	return _node, _spec
}

// SigningKeyCreateBulk is the builder for creating many SigningKey entities in bulk.
type SigningKeyCreateBulk struct {
	config
	builders []*SigningKeyCreate
}

// Save creates the SigningKey entities in the database.
func (skcb *SigningKeyCreateBulk) Save(ctx context.Context) ([]*SigningKey, error) {
	specs := make([]*sqlgraph.CreateSpec, len(skcb.builders))
	nodes := make([]*SigningKey, len(skcb.builders))
	mutators := make([]Mutator, len(skcb.builders))
	for i := range skcb.builders {
		func(i int, root context.Context) {
			builder := skcb.builders[i]
			var mut Mutator = MutateFunc(func(ctx context.Context, m Mutation) (Value, error) {
				mutation, ok := m.(*SigningKeyMutation)
				if !ok {
					return nil, fmt.Errorf("unexpected mutation type %T", m)
				}
				if err := builder.check(); err != nil {
					return nil, err
				}
				builder.mutation = mutation
				nodes[i], specs[i] = builder.createSpec()
				var err error
				if i < len(mutators)-1 {
					_, err = mutators[i+1].Mutate(root, skcb.builders[i+1].mutation)
				} else {
					spec := &sqlgraph.BatchCreateSpec{Nodes: specs}
					// Invoke the actual operation on the latest mutation in the chain.
					if err = sqlgraph.BatchCreate(ctx, skcb.driver, spec); err != nil {
						if sqlgraph.IsConstraintError(err) {
							err = &ConstraintError{msg: err.Error(), wrap: err}
						}
					}
				}
				if err != nil {
					return nil, err
				}
				mutation.id = &nodes[i].ID
				mutation.done = true
				return nodes[i], nil
			})
			for i := len(builder.hooks) - 1; i >= 0; i-- {
				mut = builder.hooks[i](mut)
			}
			mutators[i] = mut
		}(i, ctx)
	}
	if len(mutators) > 0 {
		if _, err := mutators[0].Mutate(ctx, skcb.builders[0].mutation); err != nil {
			return nil, err
		}
	}
	return nodes, nil
}

// SaveX is like Save, but panics if an error occurs.
func (skcb *SigningKeyCreateBulk) SaveX(ctx context.Context) []*SigningKey {
	v, err := skcb.Save(ctx)
	if err != nil {
		panic(err)
	}
	return v
}

// Exec executes the query.
func (skcb *SigningKeyCreateBulk) Exec(ctx context.Context) error {
	_, err := skcb.Save(ctx)
	return err
}

// ExecX is like Exec, but panics if an error occurs.
func (skcb *SigningKeyCreateBulk) ExecX(ctx context.Context) {
	if err := skcb.Exec(ctx); err != nil {
		panic(err)
	}
}
