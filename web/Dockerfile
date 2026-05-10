# 多阶段：build → nginx
FROM m.daocloud.io/docker.io/library/node:20-alpine AS build

WORKDIR /opt/web

# CN npm 镜像加速；境外用户可改回 https://registry.npmjs.org/
RUN npm config set registry https://registry.npmmirror.com

COPY package.json package-lock.json* ./
RUN npm install

COPY tsconfig.json tsconfig.node.json vite.config.ts index.html ./
COPY src ./src

RUN npm run build


FROM m.daocloud.io/docker.io/library/nginx:alpine

COPY --from=build /opt/web/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
