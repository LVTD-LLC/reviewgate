FROM node:24-alpine AS build

WORKDIR /app

COPY site/package.json site/package-lock.json ./
RUN npm ci

COPY site/ ./
RUN npm run build

FROM nginx:stable-alpine

COPY deployment/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html

EXPOSE 80
