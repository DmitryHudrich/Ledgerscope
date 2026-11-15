# syntax=docker/dockerfile:1

# Build context is the repo root.

ARG NODE_VERSION=26
ARG CADDY_VERSION=2

FROM node:${NODE_VERSION}-alpine AS build
WORKDIR /app
# Lockfile first: dependencies reinstall only when they actually change.
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
# `npm run build` typechecks before bundling, so a type error fails the image.
RUN npm run build

FROM caddy:${CADDY_VERSION}-alpine AS runtime
COPY infra/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /app/dist /srv
EXPOSE 80 443 443/udp
