#!/bin/bash
# Create GCP VMs for PA3-aligned marketplace (Option C: 5 VMs, multi-port replicas)
# Usage: ./create_vms.sh
#
# VMs: customer-stack, product-stack, seller-frontend, buyer-frontend, client-runner
#
# Default zone is us-east5-a (override with ZONE=...).
# If you see ZONE_RESOURCE_POOL_EXHAUSTED, try another zone or MACHINE_TYPE=e2-small;
# use the same ZONE for deploy_to_vms.sh / export_evaluator_env.sh.

set -e

PROJECT=$(gcloud config get-value project)
ZONE="${ZONE:-us-east5-a}"
MACHINE_TYPE="${MACHINE_TYPE:-e2-micro}"

echo "Creating VMs in project: $PROJECT, zone: $ZONE, machine-type: $MACHINE_TYPE"

for VM_NAME in \
    customer-stack product-stack seller-frontend buyer-frontend client-runner
do
    echo "Creating VM: $VM_NAME..."
    if ! gcloud compute instances create "$VM_NAME" \
        --zone="$ZONE" \
        --machine-type="$MACHINE_TYPE" \
        --image-family=ubuntu-2204-lts \
        --image-project=ubuntu-os-cloud \
        --tags=marketplace \
        --metadata=startup-script='#!/bin/bash
apt-get update
apt-get install -y protobuf-compiler'
    then
        echo "ERROR: failed to create $VM_NAME (see message above). If the VM already exists, delete it or use a different name." >&2
    fi
done

# Internal firewall: TCP for gRPC, REST, SOAP; UDP for customer atomic broadcast
# tcp 50053-50056: stacked product_db Raft peers on product-stack
echo "Creating firewall rules..."
if ! gcloud compute firewall-rules create marketplace-pa3-internal \
    --allow=tcp:50051,tcp:50052,tcp:50053,tcp:50054,tcp:50055,tcp:50056,tcp:3306,tcp:8082,tcp:8083,tcp:8084,tcp:8085,tcp:8086,tcp:8087,tcp:8088,tcp:8089,tcp:8090,udp:50000,udp:50001,udp:50002,udp:50003,udp:50004 \
    --target-tags=marketplace \
    --source-tags=marketplace
then
    echo "Note: firewall rule marketplace-pa3-internal may already exist; delete/recreate if ports are stale." >&2
fi

echo ""
echo "VMs created (5 for PA3 Option C). Get IP addresses with:"
echo "  gcloud compute instances list --filter='tags.items=marketplace'"
