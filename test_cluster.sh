#!/bin/bash

# Test script for Redis Cluster Watcher

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  Testing Redis Watcher - Cluster Mode                         ║"
echo "╚════════════════════════════════════════════════════════════════╝"

# Check if Redis Cluster is available
if [ "$REDIS_CLUSTER_AVAILABLE" != "true" ]; then
    echo "⚠️  REDIS_CLUSTER_AVAILABLE not set to 'true'"
    echo "   Skipping cluster tests"
    exit 0
fi

# Get PubSub node
PUBSUB_NODE="${REDIS_CLUSTER_PUBSUB_NODE:-redis://127.0.0.1:7000}"

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  Redis Cluster PubSub Configuration                           ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║  PubSub Node: $(printf '%-48s' "$PUBSUB_NODE") ║"
echo "║  ⚠️  All instances MUST use the SAME node for PubSub!         ║"
echo "╚════════════════════════════════════════════════════════════════╝"

echo ""
echo "Checking cluster status..."
redis-cli -c -h 127.0.0.1 -p 7000 cluster nodes || {
    echo "❌ Failed to connect to Redis Cluster"
    exit 1
}

echo ""
echo "Running cluster test..."
cargo test --lib test_redis_cluster_enforcer_sync -- --nocapture

echo ""
echo "✓ Test completed"
