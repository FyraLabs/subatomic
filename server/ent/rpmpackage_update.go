// Code generated by ent, DO NOT EDIT.

package ent

import (
	"context"
	"errors"
	"fmt"

	"entgo.io/ent/dialect/sql"
	"entgo.io/ent/dialect/sql/sqlgraph"
	"entgo.io/ent/schema/field"
	"github.com/FyraLabs/subatomic/server/ent/predicate"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/rpmpackage"
)

// RpmPackageUpdate is the builder for updating RpmPackage entities.
type RpmPackageUpdate struct {
	config
	hooks    []Hook
	mutation *RpmPackageMutation
}

// Where appends a list predicates to the RpmPackageUpdate builder.
func (rpu *RpmPackageUpdate) Where(ps ...predicate.RpmPackage) *RpmPackageUpdate {
	rpu.mutation.Where(ps...)
	return rpu
}

// SetName sets the "name" field.
func (rpu *RpmPackageUpdate) SetName(s string) *RpmPackageUpdate {
	rpu.mutation.SetName(s)
	return rpu
}

// SetEpoch sets the "epoch" field.
func (rpu *RpmPackageUpdate) SetEpoch(s string) *RpmPackageUpdate {
	rpu.mutation.SetEpoch(s)
	return rpu
}

// SetVersion sets the "version" field.
func (rpu *RpmPackageUpdate) SetVersion(s string) *RpmPackageUpdate {
	rpu.mutation.SetVersion(s)
	return rpu
}

// SetRelease sets the "release" field.
func (rpu *RpmPackageUpdate) SetRelease(s string) *RpmPackageUpdate {
	rpu.mutation.SetRelease(s)
	return rpu
}

// SetArch sets the "arch" field.
func (rpu *RpmPackageUpdate) SetArch(s string) *RpmPackageUpdate {
	rpu.mutation.SetArch(s)
	return rpu
}

// SetFilePath sets the "file_path" field.
func (rpu *RpmPackageUpdate) SetFilePath(s string) *RpmPackageUpdate {
	rpu.mutation.SetFilePath(s)
	return rpu
}

// SetRepoID sets the "repo" edge to the Repo entity by ID.
func (rpu *RpmPackageUpdate) SetRepoID(id string) *RpmPackageUpdate {
	rpu.mutation.SetRepoID(id)
	return rpu
}

// SetNillableRepoID sets the "repo" edge to the Repo entity by ID if the given value is not nil.
func (rpu *RpmPackageUpdate) SetNillableRepoID(id *string) *RpmPackageUpdate {
	if id != nil {
		rpu = rpu.SetRepoID(*id)
	}
	return rpu
}

// SetRepo sets the "repo" edge to the Repo entity.
func (rpu *RpmPackageUpdate) SetRepo(r *Repo) *RpmPackageUpdate {
	return rpu.SetRepoID(r.ID)
}

// Mutation returns the RpmPackageMutation object of the builder.
func (rpu *RpmPackageUpdate) Mutation() *RpmPackageMutation {
	return rpu.mutation
}

// ClearRepo clears the "repo" edge to the Repo entity.
func (rpu *RpmPackageUpdate) ClearRepo() *RpmPackageUpdate {
	rpu.mutation.ClearRepo()
	return rpu
}

// Save executes the query and returns the number of nodes affected by the update operation.
func (rpu *RpmPackageUpdate) Save(ctx context.Context) (int, error) {
	var (
		err      error
		affected int
	)
	if len(rpu.hooks) == 0 {
		affected, err = rpu.sqlSave(ctx)
	} else {
		var mut Mutator = MutateFunc(func(ctx context.Context, m Mutation) (Value, error) {
			mutation, ok := m.(*RpmPackageMutation)
			if !ok {
				return nil, fmt.Errorf("unexpected mutation type %T", m)
			}
			rpu.mutation = mutation
			affected, err = rpu.sqlSave(ctx)
			mutation.done = true
			return affected, err
		})
		for i := len(rpu.hooks) - 1; i >= 0; i-- {
			if rpu.hooks[i] == nil {
				return 0, fmt.Errorf("ent: uninitialized hook (forgotten import ent/runtime?)")
			}
			mut = rpu.hooks[i](mut)
		}
		if _, err := mut.Mutate(ctx, rpu.mutation); err != nil {
			return 0, err
		}
	}
	return affected, err
}

// SaveX is like Save, but panics if an error occurs.
func (rpu *RpmPackageUpdate) SaveX(ctx context.Context) int {
	affected, err := rpu.Save(ctx)
	if err != nil {
		panic(err)
	}
	return affected
}

// Exec executes the query.
func (rpu *RpmPackageUpdate) Exec(ctx context.Context) error {
	_, err := rpu.Save(ctx)
	return err
}

// ExecX is like Exec, but panics if an error occurs.
func (rpu *RpmPackageUpdate) ExecX(ctx context.Context) {
	if err := rpu.Exec(ctx); err != nil {
		panic(err)
	}
}

func (rpu *RpmPackageUpdate) sqlSave(ctx context.Context) (n int, err error) {
	_spec := &sqlgraph.UpdateSpec{
		Node: &sqlgraph.NodeSpec{
			Table:   rpmpackage.Table,
			Columns: rpmpackage.Columns,
			ID: &sqlgraph.FieldSpec{
				Type:   field.TypeInt,
				Column: rpmpackage.FieldID,
			},
		},
	}
	if ps := rpu.mutation.predicates; len(ps) > 0 {
		_spec.Predicate = func(selector *sql.Selector) {
			for i := range ps {
				ps[i](selector)
			}
		}
	}
	if value, ok := rpu.mutation.Name(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldName,
		})
	}
	if value, ok := rpu.mutation.Epoch(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldEpoch,
		})
	}
	if value, ok := rpu.mutation.Version(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldVersion,
		})
	}
	if value, ok := rpu.mutation.Release(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldRelease,
		})
	}
	if value, ok := rpu.mutation.Arch(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldArch,
		})
	}
	if value, ok := rpu.mutation.FilePath(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldFilePath,
		})
	}
	if rpu.mutation.RepoCleared() {
		edge := &sqlgraph.EdgeSpec{
			Rel:     sqlgraph.M2O,
			Inverse: true,
			Table:   rpmpackage.RepoTable,
			Columns: []string{rpmpackage.RepoColumn},
			Bidi:    false,
			Target: &sqlgraph.EdgeTarget{
				IDSpec: &sqlgraph.FieldSpec{
					Type:   field.TypeString,
					Column: repo.FieldID,
				},
			},
		}
		_spec.Edges.Clear = append(_spec.Edges.Clear, edge)
	}
	if nodes := rpu.mutation.RepoIDs(); len(nodes) > 0 {
		edge := &sqlgraph.EdgeSpec{
			Rel:     sqlgraph.M2O,
			Inverse: true,
			Table:   rpmpackage.RepoTable,
			Columns: []string{rpmpackage.RepoColumn},
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
		_spec.Edges.Add = append(_spec.Edges.Add, edge)
	}
	if n, err = sqlgraph.UpdateNodes(ctx, rpu.driver, _spec); err != nil {
		if _, ok := err.(*sqlgraph.NotFoundError); ok {
			err = &NotFoundError{rpmpackage.Label}
		} else if sqlgraph.IsConstraintError(err) {
			err = &ConstraintError{msg: err.Error(), wrap: err}
		}
		return 0, err
	}
	return n, nil
}

// RpmPackageUpdateOne is the builder for updating a single RpmPackage entity.
type RpmPackageUpdateOne struct {
	config
	fields   []string
	hooks    []Hook
	mutation *RpmPackageMutation
}

// SetName sets the "name" field.
func (rpuo *RpmPackageUpdateOne) SetName(s string) *RpmPackageUpdateOne {
	rpuo.mutation.SetName(s)
	return rpuo
}

// SetEpoch sets the "epoch" field.
func (rpuo *RpmPackageUpdateOne) SetEpoch(s string) *RpmPackageUpdateOne {
	rpuo.mutation.SetEpoch(s)
	return rpuo
}

// SetVersion sets the "version" field.
func (rpuo *RpmPackageUpdateOne) SetVersion(s string) *RpmPackageUpdateOne {
	rpuo.mutation.SetVersion(s)
	return rpuo
}

// SetRelease sets the "release" field.
func (rpuo *RpmPackageUpdateOne) SetRelease(s string) *RpmPackageUpdateOne {
	rpuo.mutation.SetRelease(s)
	return rpuo
}

// SetArch sets the "arch" field.
func (rpuo *RpmPackageUpdateOne) SetArch(s string) *RpmPackageUpdateOne {
	rpuo.mutation.SetArch(s)
	return rpuo
}

// SetFilePath sets the "file_path" field.
func (rpuo *RpmPackageUpdateOne) SetFilePath(s string) *RpmPackageUpdateOne {
	rpuo.mutation.SetFilePath(s)
	return rpuo
}

// SetRepoID sets the "repo" edge to the Repo entity by ID.
func (rpuo *RpmPackageUpdateOne) SetRepoID(id string) *RpmPackageUpdateOne {
	rpuo.mutation.SetRepoID(id)
	return rpuo
}

// SetNillableRepoID sets the "repo" edge to the Repo entity by ID if the given value is not nil.
func (rpuo *RpmPackageUpdateOne) SetNillableRepoID(id *string) *RpmPackageUpdateOne {
	if id != nil {
		rpuo = rpuo.SetRepoID(*id)
	}
	return rpuo
}

// SetRepo sets the "repo" edge to the Repo entity.
func (rpuo *RpmPackageUpdateOne) SetRepo(r *Repo) *RpmPackageUpdateOne {
	return rpuo.SetRepoID(r.ID)
}

// Mutation returns the RpmPackageMutation object of the builder.
func (rpuo *RpmPackageUpdateOne) Mutation() *RpmPackageMutation {
	return rpuo.mutation
}

// ClearRepo clears the "repo" edge to the Repo entity.
func (rpuo *RpmPackageUpdateOne) ClearRepo() *RpmPackageUpdateOne {
	rpuo.mutation.ClearRepo()
	return rpuo
}

// Select allows selecting one or more fields (columns) of the returned entity.
// The default is selecting all fields defined in the entity schema.
func (rpuo *RpmPackageUpdateOne) Select(field string, fields ...string) *RpmPackageUpdateOne {
	rpuo.fields = append([]string{field}, fields...)
	return rpuo
}

// Save executes the query and returns the updated RpmPackage entity.
func (rpuo *RpmPackageUpdateOne) Save(ctx context.Context) (*RpmPackage, error) {
	var (
		err  error
		node *RpmPackage
	)
	if len(rpuo.hooks) == 0 {
		node, err = rpuo.sqlSave(ctx)
	} else {
		var mut Mutator = MutateFunc(func(ctx context.Context, m Mutation) (Value, error) {
			mutation, ok := m.(*RpmPackageMutation)
			if !ok {
				return nil, fmt.Errorf("unexpected mutation type %T", m)
			}
			rpuo.mutation = mutation
			node, err = rpuo.sqlSave(ctx)
			mutation.done = true
			return node, err
		})
		for i := len(rpuo.hooks) - 1; i >= 0; i-- {
			if rpuo.hooks[i] == nil {
				return nil, fmt.Errorf("ent: uninitialized hook (forgotten import ent/runtime?)")
			}
			mut = rpuo.hooks[i](mut)
		}
		v, err := mut.Mutate(ctx, rpuo.mutation)
		if err != nil {
			return nil, err
		}
		nv, ok := v.(*RpmPackage)
		if !ok {
			return nil, fmt.Errorf("unexpected node type %T returned from RpmPackageMutation", v)
		}
		node = nv
	}
	return node, err
}

// SaveX is like Save, but panics if an error occurs.
func (rpuo *RpmPackageUpdateOne) SaveX(ctx context.Context) *RpmPackage {
	node, err := rpuo.Save(ctx)
	if err != nil {
		panic(err)
	}
	return node
}

// Exec executes the query on the entity.
func (rpuo *RpmPackageUpdateOne) Exec(ctx context.Context) error {
	_, err := rpuo.Save(ctx)
	return err
}

// ExecX is like Exec, but panics if an error occurs.
func (rpuo *RpmPackageUpdateOne) ExecX(ctx context.Context) {
	if err := rpuo.Exec(ctx); err != nil {
		panic(err)
	}
}

func (rpuo *RpmPackageUpdateOne) sqlSave(ctx context.Context) (_node *RpmPackage, err error) {
	_spec := &sqlgraph.UpdateSpec{
		Node: &sqlgraph.NodeSpec{
			Table:   rpmpackage.Table,
			Columns: rpmpackage.Columns,
			ID: &sqlgraph.FieldSpec{
				Type:   field.TypeInt,
				Column: rpmpackage.FieldID,
			},
		},
	}
	id, ok := rpuo.mutation.ID()
	if !ok {
		return nil, &ValidationError{Name: "id", err: errors.New(`ent: missing "RpmPackage.id" for update`)}
	}
	_spec.Node.ID.Value = id
	if fields := rpuo.fields; len(fields) > 0 {
		_spec.Node.Columns = make([]string, 0, len(fields))
		_spec.Node.Columns = append(_spec.Node.Columns, rpmpackage.FieldID)
		for _, f := range fields {
			if !rpmpackage.ValidColumn(f) {
				return nil, &ValidationError{Name: f, err: fmt.Errorf("ent: invalid field %q for query", f)}
			}
			if f != rpmpackage.FieldID {
				_spec.Node.Columns = append(_spec.Node.Columns, f)
			}
		}
	}
	if ps := rpuo.mutation.predicates; len(ps) > 0 {
		_spec.Predicate = func(selector *sql.Selector) {
			for i := range ps {
				ps[i](selector)
			}
		}
	}
	if value, ok := rpuo.mutation.Name(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldName,
		})
	}
	if value, ok := rpuo.mutation.Epoch(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldEpoch,
		})
	}
	if value, ok := rpuo.mutation.Version(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldVersion,
		})
	}
	if value, ok := rpuo.mutation.Release(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldRelease,
		})
	}
	if value, ok := rpuo.mutation.Arch(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldArch,
		})
	}
	if value, ok := rpuo.mutation.FilePath(); ok {
		_spec.Fields.Set = append(_spec.Fields.Set, &sqlgraph.FieldSpec{
			Type:   field.TypeString,
			Value:  value,
			Column: rpmpackage.FieldFilePath,
		})
	}
	if rpuo.mutation.RepoCleared() {
		edge := &sqlgraph.EdgeSpec{
			Rel:     sqlgraph.M2O,
			Inverse: true,
			Table:   rpmpackage.RepoTable,
			Columns: []string{rpmpackage.RepoColumn},
			Bidi:    false,
			Target: &sqlgraph.EdgeTarget{
				IDSpec: &sqlgraph.FieldSpec{
					Type:   field.TypeString,
					Column: repo.FieldID,
				},
			},
		}
		_spec.Edges.Clear = append(_spec.Edges.Clear, edge)
	}
	if nodes := rpuo.mutation.RepoIDs(); len(nodes) > 0 {
		edge := &sqlgraph.EdgeSpec{
			Rel:     sqlgraph.M2O,
			Inverse: true,
			Table:   rpmpackage.RepoTable,
			Columns: []string{rpmpackage.RepoColumn},
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
		_spec.Edges.Add = append(_spec.Edges.Add, edge)
	}
	_node = &RpmPackage{config: rpuo.config}
	_spec.Assign = _node.assignValues
	_spec.ScanValues = _node.scanValues
	if err = sqlgraph.UpdateNode(ctx, rpuo.driver, _spec); err != nil {
		if _, ok := err.(*sqlgraph.NotFoundError); ok {
			err = &NotFoundError{rpmpackage.Label}
		} else if sqlgraph.IsConstraintError(err) {
			err = &ConstraintError{msg: err.Error(), wrap: err}
		}
		return nil, err
	}
	return _node, nil
}