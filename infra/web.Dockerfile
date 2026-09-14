# syntax=docker/dockerfile:1

# Build context is the repo root.

ARG NODE_VERSION=26
ARG NGINX_VERSION=1.29

FROM node:${NODE_VERSION}-alpine AS build
WORKDIR /app
# Lockfile first: dependencies reinstall only when they actually change.
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
# `npm run build` typechecks before bundling, so a type error fails the image.
RUN npm run build

# Unprivileged variant runs as nginx:nginx and cannot bind ports below 1024.
FROM nginxinc/nginx-unprivileged:${NGINX_VERSION}-alpine AS runtime
# The entrypoint renders /etc/nginx/templates/*.template into conf.d at startup.
COPY infra/nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html
ENV WEB_PORT=8080 \
    API_PORT=3000 \
    NGINX_ENVSUBST_FILTER='^(WEB_PORT|API_PORT)$'
EXPOSE 8080
