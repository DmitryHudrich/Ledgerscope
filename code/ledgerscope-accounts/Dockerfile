# Ledgerscope.Accounts — public C# API (auth, cases, RBAC, SignalR graph hub).
#
# Build context: code/ledgerscope-accounts

ARG SDK_IMAGE=mcr.microsoft.com/dotnet/sdk:10.0
ARG RUNTIME_IMAGE=mcr.microsoft.com/dotnet/aspnet:10.0

FROM ${SDK_IMAGE} AS build
WORKDIR /src

# Build-time egress proxy for NuGet restore (see the note in aml-core/Dockerfile).
ARG HTTP_PROXY=""
ARG HTTPS_PROXY=""
ARG NO_PROXY="localhost,127.0.0.1"
ENV http_proxy=${HTTP_PROXY} \
    https_proxy=${HTTPS_PROXY} \
    no_proxy=${NO_PROXY} \
    HTTP_PROXY=${HTTP_PROXY} \
    HTTPS_PROXY=${HTTPS_PROXY} \
    NO_PROXY=${NO_PROXY} \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    DOTNET_NOLOGO=1

# Project files first: `dotnet restore` then re-runs only when a csproj or the
# shared props file changes, not on every source edit.
COPY Directory.Build.props ./
COPY src/Ledgerscope.Accounts.Api/Ledgerscope.Accounts.Api.csproj                       src/Ledgerscope.Accounts.Api/
COPY src/Ledgerscope.Accounts.Application/Ledgerscope.Accounts.Application.csproj       src/Ledgerscope.Accounts.Application/
COPY src/Ledgerscope.Accounts.Domain/Ledgerscope.Accounts.Domain.csproj                 src/Ledgerscope.Accounts.Domain/
COPY src/Ledgerscope.Accounts.Infrastructure/Ledgerscope.Accounts.Infrastructure.csproj src/Ledgerscope.Accounts.Infrastructure/
RUN dotnet restore src/Ledgerscope.Accounts.Api/Ledgerscope.Accounts.Api.csproj

COPY . .
RUN dotnet publish src/Ledgerscope.Accounts.Api/Ledgerscope.Accounts.Api.csproj \
        -c Release -o /app/publish --no-restore

FROM ${RUNTIME_IMAGE} AS runtime
# curl for the container healthcheck; the aspnet image ships neither curl nor wget.
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /app/publish ./
# The aspnet image already ships a non-root `app` user (UID $APP_UID); creating
# one here fails with "username already in use".
RUN chown -R app:app /app
USER app

ENV ASPNETCORE_HTTP_PORTS=8080 \
    ASPNETCORE_ENVIRONMENT=Production \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    DOTNET_NOLOGO=1
EXPOSE 8080

# `GET /` is unauthenticated and returns "Ledgerscope.Accounts".
HEALTHCHECK --interval=15s --timeout=3s --start-period=45s --retries=5 \
    CMD curl -fsS http://127.0.0.1:8080/ >/dev/null || exit 1

ENTRYPOINT ["dotnet", "Ledgerscope.Accounts.Api.dll"]
