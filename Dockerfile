FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates sudo systemd-sysv dbus \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY factorio-seasons /app/factorio-seasons
COPY static/ /app/static/
COPY migrations/ /app/migrations/
RUN mkdir -p /data/db /data/archive
RUN chmod +x /app/factorio-seasons
EXPOSE 3000
ENTRYPOINT ["/app/factorio-seasons", "/app/config.toml"]
