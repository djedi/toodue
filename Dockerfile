# Production image: static Svelte build served by the Rust binary.

FROM node:22-alpine AS frontend
WORKDIR /app
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm install
COPY frontend/ ./
RUN npm run build

FROM rust:1.88-slim AS backend
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock* ./
# Build dependencies against a stub main so they cache independently of src changes.
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN useradd --create-home toodue && mkdir -p /data && chown toodue /data
COPY --from=backend /app/target/release/toodue /usr/local/bin/toodue
COPY --from=frontend /app/dist /srv/static
ENV DATA_DIR=/data \
    STATIC_DIR=/srv/static \
    PORT=8080
VOLUME /data
EXPOSE 8080
USER toodue
CMD ["toodue"]
