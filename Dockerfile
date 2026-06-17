# ---- frontend build ----
FROM node:22-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ---- build ----
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
RUN cargo build --release --bin turn-tracker

# ---- runtime ----
FROM debian:bookworm-slim
RUN useradd -m app
WORKDIR /home/app
COPY --from=build /app/target/release/turn-tracker /usr/local/bin/turn-tracker
COPY --from=frontend /app/frontend/dist /home/app/static
ENV BIND_ADDR=0.0.0.0:8080
ENV STATIC_DIR=/home/app/static
USER app
EXPOSE 8080
CMD ["turn-tracker"]
