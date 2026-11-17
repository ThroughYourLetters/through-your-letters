FROM node:24.22.2-alpine AS builder

WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.json ./
COPY apps/web/package.json ./apps/web/package.json
COPY apps/api/config ./apps/api/config
COPY packages ./packages
COPY patches ./patches
RUN corepack enable && pnpm install --frozen-lockfile --filter @ttl/web...

COPY apps/web ./apps/web
ARG VITE_API_URL=same-origin
ARG TTL_LIMITS_FILE=../api/config/runtime.json
RUN VITE_API_URL="$VITE_API_URL" TTL_LIMITS_FILE="$TTL_LIMITS_FILE" pnpm --filter @ttl/web build

FROM nginx:alpine
COPY --from=builder /app/apps/web/dist /usr/share/nginx/html
COPY apps/web/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
