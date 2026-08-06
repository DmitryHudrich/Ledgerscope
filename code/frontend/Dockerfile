# Frontend SPA — vite build served by nginx, which also plays the role the
# vite dev proxy plays locally (see nginx.conf).
#
# Build context: code/frontend

ARG NODE_IMAGE=node:24-alpine
ARG NGINX_IMAGE=nginx:alpine

FROM ${NODE_IMAGE} AS build
WORKDIR /app

# Build-time egress proxy for `npm ci` (see the note in aml-core/Dockerfile).
ARG HTTP_PROXY=""
ARG HTTPS_PROXY=""
ARG NO_PROXY="localhost,127.0.0.1"
ENV http_proxy=${HTTP_PROXY} \
    https_proxy=${HTTPS_PROXY} \
    no_proxy=${NO_PROXY} \
    HTTP_PROXY=${HTTP_PROXY} \
    HTTPS_PROXY=${HTTPS_PROXY} \
    NO_PROXY=${NO_PROXY}

COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund

COPY . .

# Same-origin defaults: nginx below serves both paths. Override at build time
# only when the API lives on a different origin (which then needs CORS).
ARG VITE_API_URL=/api
ARG VITE_GRAPH_HUB_URL=/hubs/graph
ENV VITE_API_URL=${VITE_API_URL} \
    VITE_GRAPH_HUB_URL=${VITE_GRAPH_HUB_URL}
RUN npm run build

FROM ${NGINX_IMAGE} AS runtime
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
    CMD wget -q --spider http://127.0.0.1/ || exit 1
