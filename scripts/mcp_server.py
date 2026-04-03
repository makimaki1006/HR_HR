# -*- coding: utf-8 -*-
"""SalesNow × ハローワーク MCP Server

Claude DesktopからSalesNow企業データ + ハローワーク求人データを
自然言語で検索・分析できるMCPサーバー。

設定方法（Claude Desktop）:
claude_desktop_config.json に以下を追加:
{
  "mcpServers": {
    "salesnow-hellowork": {
      "command": "python",
      "args": ["C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/scripts/mcp_server.py"]
    }
  }
}

前提条件:
- ハローワークダッシュボードがlocalhost:9216で起動中
- pip install mcp httpx
"""
import asyncio
import json
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent

API_BASE = "http://localhost:9216/api/v1"

server = Server("salesnow-hellowork")


@server.list_tools()
async def list_tools():
    return [
        Tool(
            name="search_company",
            description="企業名で検索します。SalesNow企業データベース（19.8万社）から企業名の部分一致で検索し、従業員数・業界・都道府県・信用スコアなどの基本情報を返します。まず企業を特定するために使ってください。",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "企業名（2文字以上、例: トヨタ、京セラ、日本郵便）"
                    }
                },
                "required": ["query"]
            }
        ),
        Tool(
            name="get_company_profile",
            description="法人番号を指定して企業の詳細プロフィールと採用市場データを取得します。SalesNow企業情報（従業員数推移・売上・信用スコア）に加え、ハローワーク求人市場データ（同業界×同県の平均給与・正社員率・欠員補充率・給与帯分布）、地域統計（人口・昼夜間人口比・高齢化率）を統合して返します。search_companyで法人番号を取得してから使ってください。",
            inputSchema={
                "type": "object",
                "properties": {
                    "corporate_number": {
                        "type": "string",
                        "description": "13桁の法人番号（search_companyの結果から取得）"
                    }
                },
                "required": ["corporate_number"]
            }
        ),
        Tool(
            name="get_nearby_companies",
            description="指定企業の近隣にある企業を検索します。郵便番号の上3桁が同じエリア（半径5-15km相当）の企業を最大50社返します。各企業のハローワーク求人掲載数も表示されるので、営業先の周辺開拓や競合調査に使えます。",
            inputSchema={
                "type": "object",
                "properties": {
                    "corporate_number": {
                        "type": "string",
                        "description": "13桁の法人番号"
                    }
                },
                "required": ["corporate_number"]
            }
        ),
        Tool(
            name="get_company_postings",
            description="指定企業のハローワーク求人情報を取得します。企業名でハローワーク求人データベース（46.9万件）を検索し、職種・雇用形態・給与・勤務地などを返します。求人を出していれば採用ニーズがあると判断でき、給与水準や募集職種から企業の状況を推測できます。",
            inputSchema={
                "type": "object",
                "properties": {
                    "corporate_number": {
                        "type": "string",
                        "description": "13桁の法人番号"
                    }
                },
                "required": ["corporate_number"]
            }
        ),
        Tool(
            name="get_market_stats",
            description="特定の業界×都道府県の求人市場統計を取得します。ハローワークデータから求人数・平均給与・正社員率・欠員率・給与帯分布・福利厚生普及率などを返します。「東京都の建設業の採用市場はどうですか？」のような質問に回答できます。業界名はハローワークの13大分類（建設業、製造業、運輸業、IT・通信、サービス業、小売業、飲食業、宿泊業、医療、老人福祉・介護、教育・保育、派遣・人材、その他）を使ってください。",
            inputSchema={
                "type": "object",
                "properties": {
                    "job_type": {
                        "type": "string",
                        "description": "HW業界名（建設業、製造業、運輸業、IT・通信、サービス業、小売業、飲食業、宿泊業、医療、老人福祉・介護、教育・保育、派遣・人材、その他）"
                    },
                    "prefecture": {
                        "type": "string",
                        "description": "都道府県名（例: 東京都、大阪府、北海道）"
                    }
                },
                "required": ["job_type", "prefecture"]
            }
        ),
    ]


async def call_api(path: str, params: dict = None) -> dict:
    """REST APIを呼び出す"""
    async with httpx.AsyncClient(timeout=30.0) as client:
        try:
            resp = await client.get(f"{API_BASE}{path}", params=params)
            resp.raise_for_status()
            return resp.json()
        except httpx.ConnectError:
            return {"error": "ダッシュボードサーバー(localhost:9216)に接続できません。サーバーが起動しているか確認してください。"}
        except Exception as e:
            return {"error": f"APIエラー: {str(e)}"}


@server.call_tool()
async def call_tool(name: str, arguments: dict):
    if name == "search_company":
        data = await call_api("/companies", {"q": arguments["query"]})
    elif name == "get_company_profile":
        corp = arguments["corporate_number"]
        data = await call_api(f"/companies/{corp}")
    elif name == "get_nearby_companies":
        corp = arguments["corporate_number"]
        data = await call_api(f"/companies/{corp}/nearby")
    elif name == "get_company_postings":
        corp = arguments["corporate_number"]
        data = await call_api(f"/companies/{corp}/postings")
    elif name == "get_market_stats":
        data = await call_api("/market", {
            "job_type": arguments["job_type"],
            "prefecture": arguments["prefecture"],
        })
    else:
        data = {"error": f"不明なツール: {name}"}

    return [TextContent(type="text", text=json.dumps(data, ensure_ascii=False, indent=2))]


async def main():
    async with stdio_server() as (read, write):
        await server.run(read, write, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
