#!/bin/sh
# #NEW-134: proves an exponential-postgres image is the stock image + a working
# pgvector. Usage: infra/postgres/smoke.sh <image> <pg-major> [pgvector-version]
set -eu
image="$1"
major="$2"
want="${3:-}"
name="exp-postgres-smoke-$$"
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
docker run -d --name "$name" -e POSTGRES_PASSWORD=smoke -e POSTGRES_DB=exponential "$image" \
  -c listen_addresses='*' -c wal_level=logical >/dev/null
ok=
for _ in $(seq 60); do
  # The entrypoint restarts the server once after initdb: only a query answers "really up".
  if docker exec "$name" psql -U postgres -d exponential -tAc 'select 1' >/dev/null 2>&1; then ok=1; break; fi
  sleep 1
done
[ -n "$ok" ] || { docker logs "$name"; echo "postgres did not come up" >&2; exit 1; }
sleep 2
q() { docker exec "$name" psql -U postgres -d exponential -v ON_ERROR_STOP=1 -tAc "$1"; }
version="$(q 'show server_version_num')"
[ "$((version / 10000))" = "$major" ] || { echo "expected Postgres $major, got $version" >&2; exit 1; }
# Same libc as the stock alpine image: musl, never glibc.
docker exec "$name" sh -c 'ldd /usr/local/bin/postgres 2>&1 | grep -q musl' || { echo "not a musl build" >&2; exit 1; }
[ "$(q 'show wal_level')" = logical ]
q 'create extension vector' >/dev/null
got="$(q "select extversion from pg_extension where extname = 'vector'")"
[ -z "$want" ] || [ "$got" = "$want" ] || { echo "expected pgvector $want, got $got" >&2; exit 1; }
q "create table t (id int primary key, v vector(3)); insert into t values (1, '[1,0,0]'), (2, '[0,1,0]'), (3, '[0.9,0.1,0]'); create index on t using hnsw (v vector_cosine_ops)" >/dev/null
[ "$(q "set hnsw.iterative_scan = relaxed_order; set enable_seqscan = off; select string_agg(id::text, ',') from (select id from t order by v <=> '[1,0,0]' limit 2) s")" = "1,3" ]
echo "ok: Postgres $version (musl), pgvector $got, HNSW cosine search works"
