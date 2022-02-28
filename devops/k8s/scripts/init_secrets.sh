kubectl create secret docker-registry regcred --docker-server=registry.gitlab.com --docker-username=${GITLAB_USERNAME} --docker-password=${GITLAB_REGISTRY_TOKEN} -o yaml --dry-run --save-config | kubectl apply -f -
kubectl create secret docker-registry regcred --namespace=vod --docker-server=registry.gitlab.com --docker-username=${GITLAB_USERNAME} --docker-password=${GITLAB_REGISTRY_TOKEN} -o yaml --dry-run --save-config | kubectl apply -f -
kubectl create secret generic postgres-secret --from-literal=username=${POSTGRES_USER} --from-literal=password="${POSTGRES_PASSWORD}"
kubectl create secret generic postgres-secret --namespace=vod --from-literal=username=${POSTGRES_USER} --from-literal=password="${POSTGRES_PASSWORD}"
kubectl create secret generic fusionauth-db-secret --from-literal=username=${FUSIONAUTH_DB_USER} --from-literal=password="${FUSIONAUTH_DB_PASSWORD}"
kubectl create secret generic redshift-secret --from-literal=username=${REDSHIFT_USER} --from-literal=password="${REDSHIFT_PASSWORD}"
kubectl create secret generic fusionauth-client-secret --from-literal=devapi=${FUSIONAUTH_DEVAPI_SECRET}