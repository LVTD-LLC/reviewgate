FROM node:24-alpine AS build

WORKDIR /app

COPY site/ ./
RUN npm ci
RUN --mount=type=secret,id=posthog_key \
    if [ -f /run/secrets/posthog_key ]; then \
      export PUBLIC_POSTHOG_KEY="$(cat /run/secrets/posthog_key)"; \
    fi; \
    npm run build

FROM nginx:stable-alpine

COPY deployment/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html

EXPOSE 80
