#!/bin/bash
kubectl create namespace prometheus
helm repo add kube-state-metrics https://kubernetes.github.io/kube-state-metrics
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
helm repo add grafana https://grafana.github.io/helm-charts
helm repo update

EBS_AZ=$(kubectl get nodes \
  -o=jsonpath="{.items[0].metadata.labels['topology\.kubernetes\.io\/zone']}")

echo "
kind: StorageClass
apiVersion: storage.k8s.io/v1
metadata:
  name: prometheus
  namespace: prometheus
provisioner: ebs.csi.aws.com
parameters:
  type: gp2
reclaimPolicy: Retain
allowedTopologies:
- matchLabelExpressions:
  - key: topology.ebs.csi.aws.com/zone
    values:
    - $EBS_AZ
" | kubectl apply -f -

helm install prometheus -f prometheus_values.yml \
  prometheus-community/prometheus --namespace prometheus

helm install grafana -f grafana-values.yaml \
  grafana/grafana --namespace prometheus