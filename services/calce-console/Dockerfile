# syntax=docker/dockerfile:1.7

FROM node:22-bookworm-slim AS builder
WORKDIR /app
COPY services/calce-console/package.json services/calce-console/package-lock.json ./
RUN npm ci
COPY services/calce-console/ ./
RUN npm run build

FROM nginx:1.27-alpine AS runtime
COPY services/calce-console/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 80
