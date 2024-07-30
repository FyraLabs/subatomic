// Code generated by ent, DO NOT EDIT.

package ent

import (
	"context"
	"fmt"
	"math"

	"entgo.io/ent"
	"entgo.io/ent/dialect/sql"
	"entgo.io/ent/dialect/sql/sqlgraph"
	"entgo.io/ent/schema/field"
	"github.com/FyraLabs/subatomic/server/ent/predicate"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/rpmpackage"
)

// RpmPackageQuery is the builder for querying RpmPackage entities.
type RpmPackageQuery struct {
	config
	ctx        *QueryContext
	order      []rpmpackage.OrderOption
	inters     []Interceptor
	predicates []predicate.RpmPackage
	withRepo   *RepoQuery
	withFKs    bool
	// intermediate query (i.e. traversal path).
	sql  *sql.Selector
	path func(context.Context) (*sql.Selector, error)
}

// Where adds a new predicate for the RpmPackageQuery builder.
func (rpq *RpmPackageQuery) Where(ps ...predicate.RpmPackage) *RpmPackageQuery {
	rpq.predicates = append(rpq.predicates, ps...)
	return rpq
}

// Limit the number of records to be returned by this query.
func (rpq *RpmPackageQuery) Limit(limit int) *RpmPackageQuery {
	rpq.ctx.Limit = &limit
	return rpq
}

// Offset to start from.
func (rpq *RpmPackageQuery) Offset(offset int) *RpmPackageQuery {
	rpq.ctx.Offset = &offset
	return rpq
}

// Unique configures the query builder to filter duplicate records on query.
// By default, unique is set to true, and can be disabled using this method.
func (rpq *RpmPackageQuery) Unique(unique bool) *RpmPackageQuery {
	rpq.ctx.Unique = &unique
	return rpq
}

// Order specifies how the records should be ordered.
func (rpq *RpmPackageQuery) Order(o ...rpmpackage.OrderOption) *RpmPackageQuery {
	rpq.order = append(rpq.order, o...)
	return rpq
}

// QueryRepo chains the current query on the "repo" edge.
func (rpq *RpmPackageQuery) QueryRepo() *RepoQuery {
	query := (&RepoClient{config: rpq.config}).Query()
	query.path = func(ctx context.Context) (fromU *sql.Selector, err error) {
		if err := rpq.prepareQuery(ctx); err != nil {
			return nil, err
		}
		selector := rpq.sqlQuery(ctx)
		if err := selector.Err(); err != nil {
			return nil, err
		}
		step := sqlgraph.NewStep(
			sqlgraph.From(rpmpackage.Table, rpmpackage.FieldID, selector),
			sqlgraph.To(repo.Table, repo.FieldID),
			sqlgraph.Edge(sqlgraph.M2O, true, rpmpackage.RepoTable, rpmpackage.RepoColumn),
		)
		fromU = sqlgraph.SetNeighbors(rpq.driver.Dialect(), step)
		return fromU, nil
	}
	return query
}

// First returns the first RpmPackage entity from the query.
// Returns a *NotFoundError when no RpmPackage was found.
func (rpq *RpmPackageQuery) First(ctx context.Context) (*RpmPackage, error) {
	nodes, err := rpq.Limit(1).All(setContextOp(ctx, rpq.ctx, ent.OpQueryFirst))
	if err != nil {
		return nil, err
	}
	if len(nodes) == 0 {
		return nil, &NotFoundError{rpmpackage.Label}
	}
	return nodes[0], nil
}

// FirstX is like First, but panics if an error occurs.
func (rpq *RpmPackageQuery) FirstX(ctx context.Context) *RpmPackage {
	node, err := rpq.First(ctx)
	if err != nil && !IsNotFound(err) {
		panic(err)
	}
	return node
}

// FirstID returns the first RpmPackage ID from the query.
// Returns a *NotFoundError when no RpmPackage ID was found.
func (rpq *RpmPackageQuery) FirstID(ctx context.Context) (id int, err error) {
	var ids []int
	if ids, err = rpq.Limit(1).IDs(setContextOp(ctx, rpq.ctx, ent.OpQueryFirstID)); err != nil {
		return
	}
	if len(ids) == 0 {
		err = &NotFoundError{rpmpackage.Label}
		return
	}
	return ids[0], nil
}

// FirstIDX is like FirstID, but panics if an error occurs.
func (rpq *RpmPackageQuery) FirstIDX(ctx context.Context) int {
	id, err := rpq.FirstID(ctx)
	if err != nil && !IsNotFound(err) {
		panic(err)
	}
	return id
}

// Only returns a single RpmPackage entity found by the query, ensuring it only returns one.
// Returns a *NotSingularError when more than one RpmPackage entity is found.
// Returns a *NotFoundError when no RpmPackage entities are found.
func (rpq *RpmPackageQuery) Only(ctx context.Context) (*RpmPackage, error) {
	nodes, err := rpq.Limit(2).All(setContextOp(ctx, rpq.ctx, ent.OpQueryOnly))
	if err != nil {
		return nil, err
	}
	switch len(nodes) {
	case 1:
		return nodes[0], nil
	case 0:
		return nil, &NotFoundError{rpmpackage.Label}
	default:
		return nil, &NotSingularError{rpmpackage.Label}
	}
}

// OnlyX is like Only, but panics if an error occurs.
func (rpq *RpmPackageQuery) OnlyX(ctx context.Context) *RpmPackage {
	node, err := rpq.Only(ctx)
	if err != nil {
		panic(err)
	}
	return node
}

// OnlyID is like Only, but returns the only RpmPackage ID in the query.
// Returns a *NotSingularError when more than one RpmPackage ID is found.
// Returns a *NotFoundError when no entities are found.
func (rpq *RpmPackageQuery) OnlyID(ctx context.Context) (id int, err error) {
	var ids []int
	if ids, err = rpq.Limit(2).IDs(setContextOp(ctx, rpq.ctx, ent.OpQueryOnlyID)); err != nil {
		return
	}
	switch len(ids) {
	case 1:
		id = ids[0]
	case 0:
		err = &NotFoundError{rpmpackage.Label}
	default:
		err = &NotSingularError{rpmpackage.Label}
	}
	return
}

// OnlyIDX is like OnlyID, but panics if an error occurs.
func (rpq *RpmPackageQuery) OnlyIDX(ctx context.Context) int {
	id, err := rpq.OnlyID(ctx)
	if err != nil {
		panic(err)
	}
	return id
}

// All executes the query and returns a list of RpmPackages.
func (rpq *RpmPackageQuery) All(ctx context.Context) ([]*RpmPackage, error) {
	ctx = setContextOp(ctx, rpq.ctx, ent.OpQueryAll)
	if err := rpq.prepareQuery(ctx); err != nil {
		return nil, err
	}
	qr := querierAll[[]*RpmPackage, *RpmPackageQuery]()
	return withInterceptors[[]*RpmPackage](ctx, rpq, qr, rpq.inters)
}

// AllX is like All, but panics if an error occurs.
func (rpq *RpmPackageQuery) AllX(ctx context.Context) []*RpmPackage {
	nodes, err := rpq.All(ctx)
	if err != nil {
		panic(err)
	}
	return nodes
}

// IDs executes the query and returns a list of RpmPackage IDs.
func (rpq *RpmPackageQuery) IDs(ctx context.Context) (ids []int, err error) {
	if rpq.ctx.Unique == nil && rpq.path != nil {
		rpq.Unique(true)
	}
	ctx = setContextOp(ctx, rpq.ctx, ent.OpQueryIDs)
	if err = rpq.Select(rpmpackage.FieldID).Scan(ctx, &ids); err != nil {
		return nil, err
	}
	return ids, nil
}

// IDsX is like IDs, but panics if an error occurs.
func (rpq *RpmPackageQuery) IDsX(ctx context.Context) []int {
	ids, err := rpq.IDs(ctx)
	if err != nil {
		panic(err)
	}
	return ids
}

// Count returns the count of the given query.
func (rpq *RpmPackageQuery) Count(ctx context.Context) (int, error) {
	ctx = setContextOp(ctx, rpq.ctx, ent.OpQueryCount)
	if err := rpq.prepareQuery(ctx); err != nil {
		return 0, err
	}
	return withInterceptors[int](ctx, rpq, querierCount[*RpmPackageQuery](), rpq.inters)
}

// CountX is like Count, but panics if an error occurs.
func (rpq *RpmPackageQuery) CountX(ctx context.Context) int {
	count, err := rpq.Count(ctx)
	if err != nil {
		panic(err)
	}
	return count
}

// Exist returns true if the query has elements in the graph.
func (rpq *RpmPackageQuery) Exist(ctx context.Context) (bool, error) {
	ctx = setContextOp(ctx, rpq.ctx, ent.OpQueryExist)
	switch _, err := rpq.FirstID(ctx); {
	case IsNotFound(err):
		return false, nil
	case err != nil:
		return false, fmt.Errorf("ent: check existence: %w", err)
	default:
		return true, nil
	}
}

// ExistX is like Exist, but panics if an error occurs.
func (rpq *RpmPackageQuery) ExistX(ctx context.Context) bool {
	exist, err := rpq.Exist(ctx)
	if err != nil {
		panic(err)
	}
	return exist
}

// Clone returns a duplicate of the RpmPackageQuery builder, including all associated steps. It can be
// used to prepare common query builders and use them differently after the clone is made.
func (rpq *RpmPackageQuery) Clone() *RpmPackageQuery {
	if rpq == nil {
		return nil
	}
	return &RpmPackageQuery{
		config:     rpq.config,
		ctx:        rpq.ctx.Clone(),
		order:      append([]rpmpackage.OrderOption{}, rpq.order...),
		inters:     append([]Interceptor{}, rpq.inters...),
		predicates: append([]predicate.RpmPackage{}, rpq.predicates...),
		withRepo:   rpq.withRepo.Clone(),
		// clone intermediate query.
		sql:  rpq.sql.Clone(),
		path: rpq.path,
	}
}

// WithRepo tells the query-builder to eager-load the nodes that are connected to
// the "repo" edge. The optional arguments are used to configure the query builder of the edge.
func (rpq *RpmPackageQuery) WithRepo(opts ...func(*RepoQuery)) *RpmPackageQuery {
	query := (&RepoClient{config: rpq.config}).Query()
	for _, opt := range opts {
		opt(query)
	}
	rpq.withRepo = query
	return rpq
}

// GroupBy is used to group vertices by one or more fields/columns.
// It is often used with aggregate functions, like: count, max, mean, min, sum.
//
// Example:
//
//	var v []struct {
//		Name string `json:"name,omitempty"`
//		Count int `json:"count,omitempty"`
//	}
//
//	client.RpmPackage.Query().
//		GroupBy(rpmpackage.FieldName).
//		Aggregate(ent.Count()).
//		Scan(ctx, &v)
func (rpq *RpmPackageQuery) GroupBy(field string, fields ...string) *RpmPackageGroupBy {
	rpq.ctx.Fields = append([]string{field}, fields...)
	grbuild := &RpmPackageGroupBy{build: rpq}
	grbuild.flds = &rpq.ctx.Fields
	grbuild.label = rpmpackage.Label
	grbuild.scan = grbuild.Scan
	return grbuild
}

// Select allows the selection one or more fields/columns for the given query,
// instead of selecting all fields in the entity.
//
// Example:
//
//	var v []struct {
//		Name string `json:"name,omitempty"`
//	}
//
//	client.RpmPackage.Query().
//		Select(rpmpackage.FieldName).
//		Scan(ctx, &v)
func (rpq *RpmPackageQuery) Select(fields ...string) *RpmPackageSelect {
	rpq.ctx.Fields = append(rpq.ctx.Fields, fields...)
	sbuild := &RpmPackageSelect{RpmPackageQuery: rpq}
	sbuild.label = rpmpackage.Label
	sbuild.flds, sbuild.scan = &rpq.ctx.Fields, sbuild.Scan
	return sbuild
}

// Aggregate returns a RpmPackageSelect configured with the given aggregations.
func (rpq *RpmPackageQuery) Aggregate(fns ...AggregateFunc) *RpmPackageSelect {
	return rpq.Select().Aggregate(fns...)
}

func (rpq *RpmPackageQuery) prepareQuery(ctx context.Context) error {
	for _, inter := range rpq.inters {
		if inter == nil {
			return fmt.Errorf("ent: uninitialized interceptor (forgotten import ent/runtime?)")
		}
		if trv, ok := inter.(Traverser); ok {
			if err := trv.Traverse(ctx, rpq); err != nil {
				return err
			}
		}
	}
	for _, f := range rpq.ctx.Fields {
		if !rpmpackage.ValidColumn(f) {
			return &ValidationError{Name: f, err: fmt.Errorf("ent: invalid field %q for query", f)}
		}
	}
	if rpq.path != nil {
		prev, err := rpq.path(ctx)
		if err != nil {
			return err
		}
		rpq.sql = prev
	}
	return nil
}

func (rpq *RpmPackageQuery) sqlAll(ctx context.Context, hooks ...queryHook) ([]*RpmPackage, error) {
	var (
		nodes       = []*RpmPackage{}
		withFKs     = rpq.withFKs
		_spec       = rpq.querySpec()
		loadedTypes = [1]bool{
			rpq.withRepo != nil,
		}
	)
	if rpq.withRepo != nil {
		withFKs = true
	}
	if withFKs {
		_spec.Node.Columns = append(_spec.Node.Columns, rpmpackage.ForeignKeys...)
	}
	_spec.ScanValues = func(columns []string) ([]any, error) {
		return (*RpmPackage).scanValues(nil, columns)
	}
	_spec.Assign = func(columns []string, values []any) error {
		node := &RpmPackage{config: rpq.config}
		nodes = append(nodes, node)
		node.Edges.loadedTypes = loadedTypes
		return node.assignValues(columns, values)
	}
	for i := range hooks {
		hooks[i](ctx, _spec)
	}
	if err := sqlgraph.QueryNodes(ctx, rpq.driver, _spec); err != nil {
		return nil, err
	}
	if len(nodes) == 0 {
		return nodes, nil
	}
	if query := rpq.withRepo; query != nil {
		if err := rpq.loadRepo(ctx, query, nodes, nil,
			func(n *RpmPackage, e *Repo) { n.Edges.Repo = e }); err != nil {
			return nil, err
		}
	}
	return nodes, nil
}

func (rpq *RpmPackageQuery) loadRepo(ctx context.Context, query *RepoQuery, nodes []*RpmPackage, init func(*RpmPackage), assign func(*RpmPackage, *Repo)) error {
	ids := make([]string, 0, len(nodes))
	nodeids := make(map[string][]*RpmPackage)
	for i := range nodes {
		if nodes[i].repo_rpms == nil {
			continue
		}
		fk := *nodes[i].repo_rpms
		if _, ok := nodeids[fk]; !ok {
			ids = append(ids, fk)
		}
		nodeids[fk] = append(nodeids[fk], nodes[i])
	}
	if len(ids) == 0 {
		return nil
	}
	query.Where(repo.IDIn(ids...))
	neighbors, err := query.All(ctx)
	if err != nil {
		return err
	}
	for _, n := range neighbors {
		nodes, ok := nodeids[n.ID]
		if !ok {
			return fmt.Errorf(`unexpected foreign-key "repo_rpms" returned %v`, n.ID)
		}
		for i := range nodes {
			assign(nodes[i], n)
		}
	}
	return nil
}

func (rpq *RpmPackageQuery) sqlCount(ctx context.Context) (int, error) {
	_spec := rpq.querySpec()
	_spec.Node.Columns = rpq.ctx.Fields
	if len(rpq.ctx.Fields) > 0 {
		_spec.Unique = rpq.ctx.Unique != nil && *rpq.ctx.Unique
	}
	return sqlgraph.CountNodes(ctx, rpq.driver, _spec)
}

func (rpq *RpmPackageQuery) querySpec() *sqlgraph.QuerySpec {
	_spec := sqlgraph.NewQuerySpec(rpmpackage.Table, rpmpackage.Columns, sqlgraph.NewFieldSpec(rpmpackage.FieldID, field.TypeInt))
	_spec.From = rpq.sql
	if unique := rpq.ctx.Unique; unique != nil {
		_spec.Unique = *unique
	} else if rpq.path != nil {
		_spec.Unique = true
	}
	if fields := rpq.ctx.Fields; len(fields) > 0 {
		_spec.Node.Columns = make([]string, 0, len(fields))
		_spec.Node.Columns = append(_spec.Node.Columns, rpmpackage.FieldID)
		for i := range fields {
			if fields[i] != rpmpackage.FieldID {
				_spec.Node.Columns = append(_spec.Node.Columns, fields[i])
			}
		}
	}
	if ps := rpq.predicates; len(ps) > 0 {
		_spec.Predicate = func(selector *sql.Selector) {
			for i := range ps {
				ps[i](selector)
			}
		}
	}
	if limit := rpq.ctx.Limit; limit != nil {
		_spec.Limit = *limit
	}
	if offset := rpq.ctx.Offset; offset != nil {
		_spec.Offset = *offset
	}
	if ps := rpq.order; len(ps) > 0 {
		_spec.Order = func(selector *sql.Selector) {
			for i := range ps {
				ps[i](selector)
			}
		}
	}
	return _spec
}

func (rpq *RpmPackageQuery) sqlQuery(ctx context.Context) *sql.Selector {
	builder := sql.Dialect(rpq.driver.Dialect())
	t1 := builder.Table(rpmpackage.Table)
	columns := rpq.ctx.Fields
	if len(columns) == 0 {
		columns = rpmpackage.Columns
	}
	selector := builder.Select(t1.Columns(columns...)...).From(t1)
	if rpq.sql != nil {
		selector = rpq.sql
		selector.Select(selector.Columns(columns...)...)
	}
	if rpq.ctx.Unique != nil && *rpq.ctx.Unique {
		selector.Distinct()
	}
	for _, p := range rpq.predicates {
		p(selector)
	}
	for _, p := range rpq.order {
		p(selector)
	}
	if offset := rpq.ctx.Offset; offset != nil {
		// limit is mandatory for offset clause. We start
		// with default value, and override it below if needed.
		selector.Offset(*offset).Limit(math.MaxInt32)
	}
	if limit := rpq.ctx.Limit; limit != nil {
		selector.Limit(*limit)
	}
	return selector
}

// RpmPackageGroupBy is the group-by builder for RpmPackage entities.
type RpmPackageGroupBy struct {
	selector
	build *RpmPackageQuery
}

// Aggregate adds the given aggregation functions to the group-by query.
func (rpgb *RpmPackageGroupBy) Aggregate(fns ...AggregateFunc) *RpmPackageGroupBy {
	rpgb.fns = append(rpgb.fns, fns...)
	return rpgb
}

// Scan applies the selector query and scans the result into the given value.
func (rpgb *RpmPackageGroupBy) Scan(ctx context.Context, v any) error {
	ctx = setContextOp(ctx, rpgb.build.ctx, ent.OpQueryGroupBy)
	if err := rpgb.build.prepareQuery(ctx); err != nil {
		return err
	}
	return scanWithInterceptors[*RpmPackageQuery, *RpmPackageGroupBy](ctx, rpgb.build, rpgb, rpgb.build.inters, v)
}

func (rpgb *RpmPackageGroupBy) sqlScan(ctx context.Context, root *RpmPackageQuery, v any) error {
	selector := root.sqlQuery(ctx).Select()
	aggregation := make([]string, 0, len(rpgb.fns))
	for _, fn := range rpgb.fns {
		aggregation = append(aggregation, fn(selector))
	}
	if len(selector.SelectedColumns()) == 0 {
		columns := make([]string, 0, len(*rpgb.flds)+len(rpgb.fns))
		for _, f := range *rpgb.flds {
			columns = append(columns, selector.C(f))
		}
		columns = append(columns, aggregation...)
		selector.Select(columns...)
	}
	selector.GroupBy(selector.Columns(*rpgb.flds...)...)
	if err := selector.Err(); err != nil {
		return err
	}
	rows := &sql.Rows{}
	query, args := selector.Query()
	if err := rpgb.build.driver.Query(ctx, query, args, rows); err != nil {
		return err
	}
	defer rows.Close()
	return sql.ScanSlice(rows, v)
}

// RpmPackageSelect is the builder for selecting fields of RpmPackage entities.
type RpmPackageSelect struct {
	*RpmPackageQuery
	selector
}

// Aggregate adds the given aggregation functions to the selector query.
func (rps *RpmPackageSelect) Aggregate(fns ...AggregateFunc) *RpmPackageSelect {
	rps.fns = append(rps.fns, fns...)
	return rps
}

// Scan applies the selector query and scans the result into the given value.
func (rps *RpmPackageSelect) Scan(ctx context.Context, v any) error {
	ctx = setContextOp(ctx, rps.ctx, ent.OpQuerySelect)
	if err := rps.prepareQuery(ctx); err != nil {
		return err
	}
	return scanWithInterceptors[*RpmPackageQuery, *RpmPackageSelect](ctx, rps.RpmPackageQuery, rps, rps.inters, v)
}

func (rps *RpmPackageSelect) sqlScan(ctx context.Context, root *RpmPackageQuery, v any) error {
	selector := root.sqlQuery(ctx)
	aggregation := make([]string, 0, len(rps.fns))
	for _, fn := range rps.fns {
		aggregation = append(aggregation, fn(selector))
	}
	switch n := len(*rps.selector.flds); {
	case n == 0 && len(aggregation) > 0:
		selector.Select(aggregation...)
	case n != 0 && len(aggregation) > 0:
		selector.AppendSelect(aggregation...)
	}
	rows := &sql.Rows{}
	query, args := selector.Query()
	if err := rps.driver.Query(ctx, query, args, rows); err != nil {
		return err
	}
	defer rows.Close()
	return sql.ScanSlice(rows, v)
}
