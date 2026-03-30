#!/bin/bash

# ==========================================
# SkillStar - Development Startup Script
# ==========================================

# 1. 注入开发隔离用的环境变量
# 这样会把开发期间产生的所有配置（JSON文件等）指向 dev 目录
export SKILLSTAR_DATA_DIR="$HOME/.skillstar-dev"

# 这样会把开发期间下载的技能、克隆的仓库缓存指向 dev 目录
export SKILLSTAR_HUB_DIR="$HOME/.agents-dev"

echo "========================================="
echo "🚀 启动 SkillStar 开发隔离环境 🚀"
echo "========================================="
echo "📁 配置存储目录: $SKILLSTAR_DATA_DIR"
echo "📁 技能缓存目录: $SKILLSTAR_HUB_DIR"
echo "========================================="

# 2. 启动 Tauri 开发服务器
bun tauri dev
