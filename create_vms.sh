#!/bin/bash
# Create GCP VMs for the online marketplace
# Usage: ./create_vms.sh
#
# Creates 8 e2-micro VMs in us-central1-a (5 product_db Raft replicas + customer, seller, buyer)

set -e

PROJECT=$(gcloud config get-value project)
ZONE="us-central1-a"
MACHINE_TYPE="e2-micro"

echo "Creating VMs in project: $PROJECT, zone: $ZONE"

for VM_NAME in customer-db product-db-1 product-db-2 product-db-3 product-db-4 product-db-5 seller-server buyer-server; do
    echo "Creating VM: $VM_NAME..."
    gcloud compute instances create "$VM_NAME" \
        --zone="$ZONE" \
        --machine-type="$MACHINE_TYPE" \
        --image-family=ubuntu-2204-lts \
        --image-project=ubuntu-os-cloud \
        --tags=marketplace \
        --metadata=startup-script='#!/bin/bash
apt-get update
apt-get install -y protobuf-compiler' \
        2>/dev/null || echo "VM $VM_NAME may already exist"
done

# Create firewall rules
echo "Creating firewall rules..."
gcloud compute firewall-rules create marketplace-internal \
    --allow=tcp:50051,tcp:50052,tcp:8082,tcp:8083,tcp:8085 \
    --target-tags=marketplace \
    --source-tags=marketplace \
    2>/dev/null || echo "Firewall rule may already exist"

echo ""
echo "VMs created. Get IP addresses with:"
echo "  gcloud compute instances list --filter='tags.items=marketplace'"
