import redis.asyncio as aioredis
from app.config import settings

DEFAULT_BLOCKED_CATEGORIES = {
    "social":  ["vk.com", "ok.ru", "instagram.com", "tiktok.com", "facebook.com"],
    "games":   ["steam.com", "epicgames.com", "roblox.com", "twitch.tv"],
    "crypto":  ["binance.com", "bybit.com", "kucoin.com"],
    "adult":   [],
}

class WebFilter:
    def __init__(self):
        self.redis = aioredis.from_url(settings.REDIS_URL, decode_responses=True)

    async def is_blocked(self, domain: str) -> bool:
        return await self.redis.sismember("dlp:blocked_domains", domain)

    async def add_domain(self, domain: str):
        await self.redis.sadd("dlp:blocked_domains", domain)

    async def remove_domain(self, domain: str):
        await self.redis.srem("dlp:blocked_domains", domain)

    async def reload_from_db(self, domains: list[str]):
        await self.redis.delete("dlp:blocked_domains")
        if domains:
            await self.redis.sadd("dlp:blocked_domains", *domains)

    def categorize(self, domain: str) -> str:
        for category, domains in DEFAULT_BLOCKED_CATEGORIES.items():
            if any(domain.endswith(d) for d in domains):
                return category
        return "other"

web_filter = WebFilter()
