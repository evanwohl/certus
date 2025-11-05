#!/bin/bash
set -e

# Build Docker images for Certus nodes

echo "========================================="
echo "  Building Docker Images"
echo "========================================="

# Build node image
echo "Building certus-node image..."
docker build -t certus-node:latest ./node

# Tag for registry (if REGISTRY is set)
if [ -n "$REGISTRY" ]; then
    echo "Tagging for registry: $REGISTRY"
    docker tag certus-node:latest "$REGISTRY/certus-node:latest"
    echo "Tagged as $REGISTRY/certus-node:latest"
fi

echo ""
echo "========================================="
echo "  Docker Build Complete!"
echo "========================================="
echo ""
echo "Images built:"
echo "- certus-node:latest"
echo ""
echo "Run with docker-compose:"
echo "docker-compose up -d"