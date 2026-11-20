# ---- build stage: rewrites are compiled into routes-manifest at build time,
# so BACKEND_URL is a build arg ----
FROM node:20-slim AS build
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
ARG BACKEND_URL=http://localhost:8080
ENV NEXT_TELEMETRY_DISABLED=1 BACKEND_URL=$BACKEND_URL
RUN npm run build

# ---- runtime stage ----
FROM node:20-slim
WORKDIR /src
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 BACKEND_URL=http://backend:8080
COPY --from=build /src/package.json ./package.json
COPY --from=build /src/next.config.ts ./next.config.ts
COPY --from=build /src/tsconfig.json ./tsconfig.json
COPY --from=build /src/node_modules ./node_modules
COPY --from=build /src/.next ./.next
EXPOSE 3000
USER node
CMD ["node_modules/.bin/next", "start", "-H", "0.0.0.0", "-p", "3000"]
