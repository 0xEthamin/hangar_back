set -e

HANGAR_WEBROOT_DIR="${HANGAR_WEBROOT_DIR:-/var/www/html}"

sed "s|{{HANGAR_WEBROOT_DIR}}|${HANGAR_WEBROOT_DIR}|g" \
    /etc/nginx/http.d/default.conf.template > /etc/nginx/http.d/default.conf

echo "Nginx configured with webroot: ${HANGAR_WEBROOT_DIR}"

exec /usr/bin/supervisord -c /etc/supervisord.conf