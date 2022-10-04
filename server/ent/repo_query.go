// Code generated by ent, DO NOT EDIT.

package ent

import (
	"context"
	"database/sql/driver"
	"fmt"
	"math"

	"entgo.io/ent/dialect/sql"
	"entgo.io/ent/dialect/sql/sqlgraph"
	"entgo.io/ent/schema/field"
	"github.com/FyraLabs/subatomic/server/ent/predicate"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/rpmpackage"
	"github.com/FyraLabs/subatomic/server/ent/signingkey"
)

// RepoQuery is the builder for querying Repo entities.
type RepoQuery struct {
	config
	limit      *int
	offset     *int
	unique     *bool
	order      []OrderFunc
	fields     []string
	predicates []predicate.Repo
	withRpms   *RpmPackageQuery
	withKey    *SigningKeyQuery
	withFKs    bool
	// intermediate query (i.e. traversal path).
	sql  *sql.Selector
	path func(context.Context) (*sql.Selector, error)
}

// Where adds a new predicate for the RepoQuery builder.
func (rq *RepoQuery) Where(ps ...predicate.Repo) *RepoQuery {
	rq.predicates = append(rq.predicates, ps...)
	return rq
}

// Limit adds a limit step to the query.
func (rq *RepoQuery) Limit(limit int) *RepoQuery {
	rq.limit = &limit
	return rq
}

// Offset adds an offset step to the query.
func (rq *RepoQuery) Offset(offset int) *RepoQuery {
	rq.offset = &offset
	return rq
}

// Unique configures the query builder to filter duplicate records on query.
// By default, unique is set to true, and can be disabled using this method.
func (rq *RepoQuery) Unique(unique bool) *RepoQuery {
	rq.unique = &unique
	return rq
}

// Order adds an order step to the query.
func (rq *RepoQuery) Order(o ...OrderFunc) *RepoQuery {
	rq.order = append(rq.order, o...)
	return rq
}

// QueryRpms chains the current query on the "rpms" edge.
func (rq *RepoQuery) QueryRpms() *RpmPackageQuery {
	query := &RpmPackageQuery{config: rq.config}
	query.path = func(ctx context.Context) (fromU *sql.Selector, err error) {
		if err := rq.prepareQuery(ctx); err != nil {
			return nil, err
		}
		selector := rq.sqlQuery(ctx)
		if err := selector.Err(); err != nil {
			return nil, err
		}
		step := sqlgraph.NewStep(
			sqlgraph.From(repo.Table, repo.FieldID, selector),
			sqlgraph.To(rpmpackage.Table, rpmpackage.FieldID),
			sqlgraph.Edge(sqlgraph.O2M, false, repo.RpmsTable, repo.RpmsColumn),
		)
		fromU = sqlgraph.SetNeighbors(rq.driver.Dialect(), step)
		return fromU, nil
	}
	return query
}

// QueryKey chains the current query on the "key" edge.
func (rq *RepoQuery) QueryKey() *SigningKeyQuery {
	query := &SigningKeyQuery{config: rq.config}
	query.path = func(ctx context.Context) (fromU *sql.Selector, err error) {
		if err := rq.prepareQuery(ctx); err != nil {
			return nil, err
		}
		selector := rq.sqlQuery(ctx)
		if err := selector.Err(); err != nil {
			return nil, err
		}
		step := sqlgraph.NewStep(
			sqlgraph.From(repo.Table, repo.FieldID, selector),
			sqlgraph.To(signingkey.Table, signingkey.FieldID),
			sqlgraph.Edge(sqlgraph.M2O, false, repo.KeyTable, repo.KeyColumn),
		)
		fromU = sqlgraph.SetNeighbors(rq.driver.Dialect(), step)
		return fromU, nil
	}
	return query
}

// First returns the first Repo entity from the query.
// Returns a *NotFoundError when no Repo was found.
func (rq *RepoQuery) First(ctx context.Context) (*Repo, error) {
	nodes, err := rq.Limit(1).All(ctx)
	if err != nil {
		return nil, err
	}
	if len(nodes) == 0 {
		return nil, &NotFoundError{repo.Label}
	}
	return nodes[0], nil
}

// FirstX is like First, but panics if an error occurs.
func (rq *RepoQuery) FirstX(ctx context.Context) *Repo {
	node, err := rq.First(ctx)
	if err != nil && !IsNotFound(err) {
		panic(err)
	}
	return node
}

// FirstID returns the first Repo ID from the query.
// Returns a *NotFoundError when no Repo ID was found.
func (rq *RepoQuery) FirstID(ctx context.Context) (id string, err error) {
	var ids []string
	if ids, err = rq.Limit(1).IDs(ctx); err != nil {
		return
	}
	if len(ids) == 0 {
		err = &NotFoundError{repo.Label}
		return
	}
	return ids[0], nil
}

// FirstIDX is like FirstID, but panics if an error occurs.
func (rq *RepoQuery) FirstIDX(ctx context.Context) string {
	id, err := rq.FirstID(ctx)
	if err != nil && !IsNotFound(err) {
		panic(err)
	}
	return id
}

// Only returns a single Repo entity found by the query, ensuring it only returns one.
// Returns a *NotSingularError when more than one Repo entity is found.
// Returns a *NotFoundError when no Repo entities are found.
func (rq *RepoQuery) Only(ctx context.Context) (*Repo, error) {
	nodes, err := rq.Limit(2).All(ctx)
	if err != nil {
		return nil, err
	}
	switch len(nodes) {
	case 1:
		return nodes[0], nil
	case 0:
		return nil, &NotFoundError{repo.Label}
	default:
		return nil, &NotSingularError{repo.Label}
	}
}

// OnlyX is like Only, but panics if an error occurs.
func (rq *RepoQuery) OnlyX(ctx context.Context) *Repo {
	node, err := rq.Only(ctx)
	if err != nil {
		panic(err)
	}
	return node
}

// OnlyID is like Only, but returns the only Repo ID in the query.
// Returns a *NotSingularError when more than one Repo ID is found.
// Returns a *NotFoundError when no entities are found.
func (rq *RepoQuery) OnlyID(ctx context.Context) (id string, err error) {
	var ids []string
	if ids, err = rq.Limit(2).IDs(ctx); err != nil {
		return
	}
	switch len(ids) {
	case 1:
		id = ids[0]
	case 0:
		err = &NotFoundError{repo.Label}
	default:
		err = &NotSingularError{repo.Label}
	}
	return
}

// OnlyIDX is like OnlyID, but panics if an error occurs.
func (rq *RepoQuery) OnlyIDX(ctx context.Context) string {
	id, err := rq.OnlyID(ctx)
	if err != nil {
		panic(err)
	}
	return id
}

// All executes the query and returns a list of Repos.
func (rq *RepoQuery) All(ctx context.Context) ([]*Repo, error) {
	if err := rq.prepareQuery(ctx); err != nil {
		return nil, err
	}
	return rq.sqlAll(ctx)
}

// AllX is like All, but panics if an error occurs.
func (rq *RepoQuery) AllX(ctx context.Context) []*Repo {
	nodes, err := rq.All(ctx)
	if err != nil {
		panic(err)
	}
	return nodes
}

// IDs executes the query and returns a list of Repo IDs.
func (rq *RepoQuery) IDs(ctx context.Context) ([]string, error) {
	var ids []string
	if err := rq.Select(repo.FieldID).Scan(ctx, &ids); err != nil {
		return nil, err
	}
	return ids, nil
}

// IDsX is like IDs, but panics if an error occurs.
func (rq *RepoQuery) IDsX(ctx context.Context) []string {
	ids, err := rq.IDs(ctx)
	if err != nil {
		panic(err)
	}
	return ids
}

// Count returns the count of the given query.
func (rq *RepoQuery) Count(ctx context.Context) (int, error) {
	if err := rq.prepareQuery(ctx); err != nil {
		return 0, err
	}
	return rq.sqlCount(ctx)
}

// CountX is like Count, but panics if an error occurs.
func (rq *RepoQuery) CountX(ctx context.Context) int {
	count, err := rq.Count(ctx)
	if err != nil {
		panic(err)
	}
	return count
}

// Exist returns true if the query has elements in the graph.
func (rq *RepoQuery) Exist(ctx context.Context) (bool, error) {
	if err := rq.prepareQuery(ctx); err != nil {
		return false, err
	}
	return rq.sqlExist(ctx)
}

// ExistX is like Exist, but panics if an error occurs.
func (rq *RepoQuery) ExistX(ctx context.Context) bool {
	exist, err := rq.Exist(ctx)
	if err != nil {
		panic(err)
	}
	return exist
}

// Clone returns a duplicate of the RepoQuery builder, including all associated steps. It can be
// used to prepare common query builders and use them differently after the clone is made.
func (rq *RepoQuery) Clone() *RepoQuery {
	if rq == nil {
		return nil
	}
	return &RepoQuery{
		config:     rq.config,
		limit:      rq.limit,
		offset:     rq.offset,
		order:      append([]OrderFunc{}, rq.order...),
		predicates: append([]predicate.Repo{}, rq.predicates...),
		withRpms:   rq.withRpms.Clone(),
		withKey:    rq.withKey.Clone(),
		// clone intermediate query.
		sql:    rq.sql.Clone(),
		path:   rq.path,
		unique: rq.unique,
	}
}

// WithRpms tells the query-builder to eager-load the nodes that are connected to
// the "rpms" edge. The optional arguments are used to configure the query builder of the edge.
func (rq *RepoQuery) WithRpms(opts ...func(*RpmPackageQuery)) *RepoQuery {
	query := &RpmPackageQuery{config: rq.config}
	for _, opt := range opts {
		opt(query)
	}
	rq.withRpms = query
	return rq
}

// WithKey tells the query-builder to eager-load the nodes that are connected to
// the "key" edge. The optional arguments are used to configure the query builder of the edge.
func (rq *RepoQuery) WithKey(opts ...func(*SigningKeyQuery)) *RepoQuery {
	query := &SigningKeyQuery{config: rq.config}
	for _, opt := range opts {
		opt(query)
	}
	rq.withKey = query
	return rq
}

// GroupBy is used to group vertices by one or more fields/columns.
// It is often used with aggregate functions, like: count, max, mean, min, sum.
//
// Example:
//
//	var v []struct {
//		Type repo.Type `json:"type,omitempty"`
//		Count int `json:"count,omitempty"`
//	}
//
//	client.Repo.Query().
//		GroupBy(repo.FieldType).
//		Aggregate(ent.Count()).
//		Scan(ctx, &v)
func (rq *RepoQuery) GroupBy(field string, fields ...string) *RepoGroupBy {
	grbuild := &RepoGroupBy{config: rq.config}
	grbuild.fields = append([]string{field}, fields...)
	grbuild.path = func(ctx context.Context) (prev *sql.Selector, err error) {
		if err := rq.prepareQuery(ctx); err != nil {
			return nil, err
		}
		return rq.sqlQuery(ctx), nil
	}
	grbuild.label = repo.Label
	grbuild.flds, grbuild.scan = &grbuild.fields, grbuild.Scan
	return grbuild
}

// Select allows the selection one or more fields/columns for the given query,
// instead of selecting all fields in the entity.
//
// Example:
//
//	var v []struct {
//		Type repo.Type `json:"type,omitempty"`
//	}
//
//	client.Repo.Query().
//		Select(repo.FieldType).
//		Scan(ctx, &v)
func (rq *RepoQuery) Select(fields ...string) *RepoSelect {
	rq.fields = append(rq.fields, fields...)
	selbuild := &RepoSelect{RepoQuery: rq}
	selbuild.label = repo.Label
	selbuild.flds, selbuild.scan = &rq.fields, selbuild.Scan
	return selbuild
}

func (rq *RepoQuery) prepareQuery(ctx context.Context) error {
	for _, f := range rq.fields {
		if !repo.ValidColumn(f) {
			return &ValidationError{Name: f, err: fmt.Errorf("ent: invalid field %q for query", f)}
		}
	}
	if rq.path != nil {
		prev, err := rq.path(ctx)
		if err != nil {
			return err
		}
		rq.sql = prev
	}
	return nil
}

func (rq *RepoQuery) sqlAll(ctx context.Context, hooks ...queryHook) ([]*Repo, error) {
	var (
		nodes       = []*Repo{}
		withFKs     = rq.withFKs
		_spec       = rq.querySpec()
		loadedTypes = [2]bool{
			rq.withRpms != nil,
			rq.withKey != nil,
		}
	)
	if rq.withKey != nil {
		withFKs = true
	}
	if withFKs {
		_spec.Node.Columns = append(_spec.Node.Columns, repo.ForeignKeys...)
	}
	_spec.ScanValues = func(columns []string) ([]interface{}, error) {
		return (*Repo).scanValues(nil, columns)
	}
	_spec.Assign = func(columns []string, values []interface{}) error {
		node := &Repo{config: rq.config}
		nodes = append(nodes, node)
		node.Edges.loadedTypes = loadedTypes
		return node.assignValues(columns, values)
	}
	for i := range hooks {
		hooks[i](ctx, _spec)
	}
	if err := sqlgraph.QueryNodes(ctx, rq.driver, _spec); err != nil {
		return nil, err
	}
	if len(nodes) == 0 {
		return nodes, nil
	}
	if query := rq.withRpms; query != nil {
		if err := rq.loadRpms(ctx, query, nodes,
			func(n *Repo) { n.Edges.Rpms = []*RpmPackage{} },
			func(n *Repo, e *RpmPackage) { n.Edges.Rpms = append(n.Edges.Rpms, e) }); err != nil {
			return nil, err
		}
	}
	if query := rq.withKey; query != nil {
		if err := rq.loadKey(ctx, query, nodes, nil,
			func(n *Repo, e *SigningKey) { n.Edges.Key = e }); err != nil {
			return nil, err
		}
	}
	return nodes, nil
}

func (rq *RepoQuery) loadRpms(ctx context.Context, query *RpmPackageQuery, nodes []*Repo, init func(*Repo), assign func(*Repo, *RpmPackage)) error {
	fks := make([]driver.Value, 0, len(nodes))
	nodeids := make(map[string]*Repo)
	for i := range nodes {
		fks = append(fks, nodes[i].ID)
		nodeids[nodes[i].ID] = nodes[i]
		if init != nil {
			init(nodes[i])
		}
	}
	query.withFKs = true
	query.Where(predicate.RpmPackage(func(s *sql.Selector) {
		s.Where(sql.InValues(repo.RpmsColumn, fks...))
	}))
	neighbors, err := query.All(ctx)
	if err != nil {
		return err
	}
	for _, n := range neighbors {
		fk := n.repo_rpms
		if fk == nil {
			return fmt.Errorf(`foreign-key "repo_rpms" is nil for node %v`, n.ID)
		}
		node, ok := nodeids[*fk]
		if !ok {
			return fmt.Errorf(`unexpected foreign-key "repo_rpms" returned %v for node %v`, *fk, n.ID)
		}
		assign(node, n)
	}
	return nil
}
func (rq *RepoQuery) loadKey(ctx context.Context, query *SigningKeyQuery, nodes []*Repo, init func(*Repo), assign func(*Repo, *SigningKey)) error {
	ids := make([]string, 0, len(nodes))
	nodeids := make(map[string][]*Repo)
	for i := range nodes {
		if nodes[i].repo_key == nil {
			continue
		}
		fk := *nodes[i].repo_key
		if _, ok := nodeids[fk]; !ok {
			ids = append(ids, fk)
		}
		nodeids[fk] = append(nodeids[fk], nodes[i])
	}
	query.Where(signingkey.IDIn(ids...))
	neighbors, err := query.All(ctx)
	if err != nil {
		return err
	}
	for _, n := range neighbors {
		nodes, ok := nodeids[n.ID]
		if !ok {
			return fmt.Errorf(`unexpected foreign-key "repo_key" returned %v`, n.ID)
		}
		for i := range nodes {
			assign(nodes[i], n)
		}
	}
	return nil
}

func (rq *RepoQuery) sqlCount(ctx context.Context) (int, error) {
	_spec := rq.querySpec()
	_spec.Node.Columns = rq.fields
	if len(rq.fields) > 0 {
		_spec.Unique = rq.unique != nil && *rq.unique
	}
	return sqlgraph.CountNodes(ctx, rq.driver, _spec)
}

func (rq *RepoQuery) sqlExist(ctx context.Context) (bool, error) {
	n, err := rq.sqlCount(ctx)
	if err != nil {
		return false, fmt.Errorf("ent: check existence: %w", err)
	}
	return n > 0, nil
}

func (rq *RepoQuery) querySpec() *sqlgraph.QuerySpec {
	_spec := &sqlgraph.QuerySpec{
		Node: &sqlgraph.NodeSpec{
			Table:   repo.Table,
			Columns: repo.Columns,
			ID: &sqlgraph.FieldSpec{
				Type:   field.TypeString,
				Column: repo.FieldID,
			},
		},
		From:   rq.sql,
		Unique: true,
	}
	if unique := rq.unique; unique != nil {
		_spec.Unique = *unique
	}
	if fields := rq.fields; len(fields) > 0 {
		_spec.Node.Columns = make([]string, 0, len(fields))
		_spec.Node.Columns = append(_spec.Node.Columns, repo.FieldID)
		for i := range fields {
			if fields[i] != repo.FieldID {
				_spec.Node.Columns = append(_spec.Node.Columns, fields[i])
			}
		}
	}
	if ps := rq.predicates; len(ps) > 0 {
		_spec.Predicate = func(selector *sql.Selector) {
			for i := range ps {
				ps[i](selector)
			}
		}
	}
	if limit := rq.limit; limit != nil {
		_spec.Limit = *limit
	}
	if offset := rq.offset; offset != nil {
		_spec.Offset = *offset
	}
	if ps := rq.order; len(ps) > 0 {
		_spec.Order = func(selector *sql.Selector) {
			for i := range ps {
				ps[i](selector)
			}
		}
	}
	return _spec
}

func (rq *RepoQuery) sqlQuery(ctx context.Context) *sql.Selector {
	builder := sql.Dialect(rq.driver.Dialect())
	t1 := builder.Table(repo.Table)
	columns := rq.fields
	if len(columns) == 0 {
		columns = repo.Columns
	}
	selector := builder.Select(t1.Columns(columns...)...).From(t1)
	if rq.sql != nil {
		selector = rq.sql
		selector.Select(selector.Columns(columns...)...)
	}
	if rq.unique != nil && *rq.unique {
		selector.Distinct()
	}
	for _, p := range rq.predicates {
		p(selector)
	}
	for _, p := range rq.order {
		p(selector)
	}
	if offset := rq.offset; offset != nil {
		// limit is mandatory for offset clause. We start
		// with default value, and override it below if needed.
		selector.Offset(*offset).Limit(math.MaxInt32)
	}
	if limit := rq.limit; limit != nil {
		selector.Limit(*limit)
	}
	return selector
}

// RepoGroupBy is the group-by builder for Repo entities.
type RepoGroupBy struct {
	config
	selector
	fields []string
	fns    []AggregateFunc
	// intermediate query (i.e. traversal path).
	sql  *sql.Selector
	path func(context.Context) (*sql.Selector, error)
}

// Aggregate adds the given aggregation functions to the group-by query.
func (rgb *RepoGroupBy) Aggregate(fns ...AggregateFunc) *RepoGroupBy {
	rgb.fns = append(rgb.fns, fns...)
	return rgb
}

// Scan applies the group-by query and scans the result into the given value.
func (rgb *RepoGroupBy) Scan(ctx context.Context, v interface{}) error {
	query, err := rgb.path(ctx)
	if err != nil {
		return err
	}
	rgb.sql = query
	return rgb.sqlScan(ctx, v)
}

func (rgb *RepoGroupBy) sqlScan(ctx context.Context, v interface{}) error {
	for _, f := range rgb.fields {
		if !repo.ValidColumn(f) {
			return &ValidationError{Name: f, err: fmt.Errorf("invalid field %q for group-by", f)}
		}
	}
	selector := rgb.sqlQuery()
	if err := selector.Err(); err != nil {
		return err
	}
	rows := &sql.Rows{}
	query, args := selector.Query()
	if err := rgb.driver.Query(ctx, query, args, rows); err != nil {
		return err
	}
	defer rows.Close()
	return sql.ScanSlice(rows, v)
}

func (rgb *RepoGroupBy) sqlQuery() *sql.Selector {
	selector := rgb.sql.Select()
	aggregation := make([]string, 0, len(rgb.fns))
	for _, fn := range rgb.fns {
		aggregation = append(aggregation, fn(selector))
	}
	// If no columns were selected in a custom aggregation function, the default
	// selection is the fields used for "group-by", and the aggregation functions.
	if len(selector.SelectedColumns()) == 0 {
		columns := make([]string, 0, len(rgb.fields)+len(rgb.fns))
		for _, f := range rgb.fields {
			columns = append(columns, selector.C(f))
		}
		columns = append(columns, aggregation...)
		selector.Select(columns...)
	}
	return selector.GroupBy(selector.Columns(rgb.fields...)...)
}

// RepoSelect is the builder for selecting fields of Repo entities.
type RepoSelect struct {
	*RepoQuery
	selector
	// intermediate query (i.e. traversal path).
	sql *sql.Selector
}

// Scan applies the selector query and scans the result into the given value.
func (rs *RepoSelect) Scan(ctx context.Context, v interface{}) error {
	if err := rs.prepareQuery(ctx); err != nil {
		return err
	}
	rs.sql = rs.RepoQuery.sqlQuery(ctx)
	return rs.sqlScan(ctx, v)
}

func (rs *RepoSelect) sqlScan(ctx context.Context, v interface{}) error {
	rows := &sql.Rows{}
	query, args := rs.sql.Query()
	if err := rs.driver.Query(ctx, query, args, rows); err != nil {
		return err
	}
	defer rows.Close()
	return sql.ScanSlice(rows, v)
}
