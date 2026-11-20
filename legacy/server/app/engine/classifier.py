import re
from dataclasses import dataclass

@dataclass
class Match:
    rule_name: str
    severity: str
    snippet: str

PATTERNS = {
    "ИИН (Казахстан)": {
        "regex": r'\b\d{12}\b',
        "severity": "critical"
    },
    "БИН (Казахстан)": {
        "regex": r'\b[0-9]{12}\b',
        "severity": "high"
    },
    "Номер банковской карты": {
        "regex": r'\b(?:\d[ \-]?){13,19}\b',
        "severity": "critical"
    },
    "Паспорт РК": {
        "regex": r'\b[A-ZА-Я]{1,2}\d{7}\b',
        "severity": "high"
    },
    "Телефон КЗ": {
        "regex": r'(\+7|8)[\s\-]?\(?\d{3}\)?[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}',
        "severity": "medium"
    },
    "IBAN счёт": {
        "regex": r'\bKZ\d{2}[A-Z0-9]{16}\b',
        "severity": "critical"
    },
    "Email адрес": {
        "regex": r'\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b',
        "severity": "low"
    },
    "IP адрес (внутренний)": {
        "regex": r'\b(192\.168|10\.\d{1,3}|172\.(1[6-9]|2\d|3[01]))\.\d{1,3}\.\d{1,3}\b',
        "severity": "medium"
    },
    "Ключевые слова конфиденциальности": {
        "keywords": [
            "конфиденциально", "секретно", "для служебного пользования",
            "не для распространения", "коммерческая тайна", "только для внутреннего",
            "строго конфиденциально", "құпия", "жасырын"
        ],
        "severity": "high"
    },
    "Финансовые ключевые слова": {
        "keywords": [
            "зарплата", "оклад", "платёжная ведомость", "бюджет компании",
            "финансовый отчёт", "выручка", "прибыль до налогов"
        ],
        "severity": "medium"
    },
}

class DataClassifier:
    def analyze(self, text: str) -> list[Match]:
        if not text:
            return []

        matches = []
        text_lower = text.lower()

        for rule_name, rule in PATTERNS.items():
            if "regex" in rule:
                for m in re.finditer(rule["regex"], text):
                    start = max(0, m.start() - 20)
                    end = min(len(text), m.end() + 20)
                    snippet = f"...{text[start:end]}..."
                    matches.append(Match(
                        rule_name=rule_name,
                        severity=rule["severity"],
                        snippet=snippet
                    ))
            elif "keywords" in rule:
                for kw in rule["keywords"]:
                    if kw in text_lower:
                        matches.append(Match(
                            rule_name=rule_name,
                            severity=rule["severity"],
                            snippet=kw
                        ))

        return matches

    def max_severity(self, matches: list[Match]) -> str:
        order = {"low": 1, "medium": 2, "high": 3, "critical": 4}
        if not matches:
            return "low"
        return max(matches, key=lambda m: order.get(m.severity, 0)).severity

classifier = DataClassifier()
