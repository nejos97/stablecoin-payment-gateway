# Build the SPA
FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build

# Serve with nginx: static SPA + /api proxied to the backend named by API_URL
# (rendered from the template at container startup by the image's envsubst hook)
FROM nginx:1.27-alpine
ENV API_URL=http://api:3000
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
