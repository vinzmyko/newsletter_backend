#!/usr/bin/env bash
set -x
set -eo pipefail

# Check if psql is installed
if ! [ -x "$(command -v psql)" ]; then
    echo >&2 "Error: psql is not installed."
    exit 1
fi

# Check if sqlx is installed
if ! [ -x "$(command -v sqlx)" ]; then
    echo >&2 "Error: sqlx is not installed."
    echo >&2 "Use:"
    echo >&2 "  cargo install sqlx-cli --version=0.6.3 --no-default-features --features rustls,postgres"
    echo >&2 "to install it."
    exit 1
fi

# Check if variable has been set, else default to another value
DB_USER=${POSTGRES_USER:=postgres}
DB_PASSWORD="${POSTGRES_PASSWORD:=password}"
DB_NAME="${POSTGRES_DB:=newsletter}"
DB_PORT="${POSTGRES_PORT:=5432}"

# Only start Docker container if not skipping Docker
if [[ -z "${SKIP_DOCKER}" ]]
then
docker run \
    -e POSTGRES_USER=${DB_USER} \
    -e POSTGRES_PASSWORD=${DB_PASSWORD} \
    -e POSTGRES_DB=${DB_NAME} \
    -p "${DB_PORT}":5432 \
    -d postgres \
    postgres -N 1000
fi

# Keep pinging Postgres until it's ready to accept commands
export PGPASSWORD="${DB_PASSWORD}"
until psql -h "localhost" -U "${DB_USER}" -p "${DB_PORT}" -d "postgres" -c '\q'; do
    >&2 echo "Postgres is still unavailable - sleeping"
    sleep 1
done

>&2 echo "Postgres is up and running on port ${DB_PORT}!"

# Set the database URL for sqlx commands
export DATABASE_URL=postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_PORT}/${DB_NAME}

# Check if the database already exists using psql
if psql -h "localhost" -U "${DB_USER}" -p "${DB_PORT}" -lqt | cut -d \| -f 1 | grep -qw "${DB_NAME}"; then
    echo "Database '${DB_NAME}' already exists. Skipping creation."
else
    echo "Database '${DB_NAME}' does not exist. Creating..."
    sqlx database create
    echo "Database '${DB_NAME}' created."
fi

sqlx migrate run

>&2 echo "Postgres has been migrated, ready to go!"
