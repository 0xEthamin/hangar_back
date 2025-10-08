FROM php:8.4-fpm-alpine3.22@sha256:1f684fd5ae2fe39114d3e4fb562e31a0bf4866ec10dc1902c6462d55f4707735

LABEL maintainer="dsi@garageisep.com"
LABEL description="Serveur PHP-FPM + Nginx all-in-one"

RUN apk update && apk upgrade --no-cache && rm -rf /var/cache/apk/*

RUN apk add --no-cache \
        nginx \
        supervisor \
        libpng-dev \
        libjpeg-turbo-dev \
        freetype-dev \
        libzip-dev \
        icu-dev \
    && docker-php-ext-configure gd --with-freetype --with-jpeg \
    && docker-php-ext-install -j$(nproc) \
        gd mysqli pdo pdo_mysql opcache zip intl \
    && rm -rf /tmp/*

RUN { \
    echo 'opcache.enable=1'; \
    echo 'opcache.memory_consumption=128'; \
    echo 'opcache.interned_strings_buffer=8'; \
    echo 'opcache.max_accelerated_files=4000'; \
    echo 'opcache.revalidate_freq=60'; \
    } > /usr/local/etc/php/conf.d/opcache.ini

RUN { \
    echo 'memory_limit=128M'; \
    echo 'upload_max_filesize=50M'; \
    echo 'post_max_size=50M'; \
    echo 'max_execution_time=60'; \
    echo 'expose_php=Off'; \
    echo 'display_errors=Off'; \
    } > /usr/local/etc/php/conf.d/custom.ini

RUN { \
    echo '[global]'; \
    echo 'daemonize = no'; \
    echo; \
    echo '[www]'; \
    echo 'user = appuser'; \
    echo 'group = appgroup'; \
    echo 'listen = 127.0.0.1:9000'; \
    echo 'listen.owner = appuser'; \
    echo 'listen.group = appgroup'; \
    echo 'pm = dynamic'; \
    echo 'pm.max_children = 10'; \
    echo 'pm.start_servers = 2'; \
    echo 'pm.min_spare_servers = 1'; \
    echo 'pm.max_spare_servers = 3'; \
    echo 'pm.max_requests = 500'; \
    } > /usr/local/etc/php-fpm.d/zz-docker.conf


COPY config/nginx.conf /etc/nginx/nginx.conf
COPY config/default.conf /etc/nginx/http.d/default.conf
COPY config/supervisord.conf /etc/supervisord.conf

RUN addgroup -g 1000 appgroup && \
    adduser -u 1000 -D -G appgroup appuser

RUN rm -rf /var/www/html && \
    mkdir -p /var/www/html

RUN mkdir -p /run/nginx && \
    chown -R appuser:appgroup /var/www/html /var/lib/nginx /var/log/nginx /run/nginx && \
    chmod -R 755 /var/www/html

RUN rm /usr/local/etc/php-fpm.d/www.conf.default

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost/ || exit 1

USER appuser

ENTRYPOINT ["/usr/bin/supervisord", "-c", "/etc/supervisord.conf"]