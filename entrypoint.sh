#!/bin/sh

# Use this script as the entrypoint for Docker container
# exit when any command fails
set -e

if [ -f "$DB_PASSWORD_FILE" ]; then
  passwd=$(cat "$DB_PASSWORD_FILE")
  export DATABASE_URL=$(echo "$DATABASE_URL" | sed "s/hilo_pass/$passwd/")
else
  echo "[WARN] DB_PASSWORD_FILE not found, using default from DATABASE_URL if available. Do not use in production!"
fi


exec "$@"
