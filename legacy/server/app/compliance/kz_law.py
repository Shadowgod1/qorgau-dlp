"""
Модуль соответствия законодательству Республики Казахстан.

Основные НПА:
- Закон РК "О персональных данных и их защите" от 21.05.2013 №94-V
- Трудовой кодекс РК (ст. 23 — уведомление работников о слежении)
- Закон РК "Об информационной безопасности" от 2022 года
- Приказ МЦРиАП РК №410 — требования к ИБ
"""

from dataclasses import dataclass
from typing import Optional

@dataclass
class ComplianceCheck:
    compliant: bool
    violations: list[str]
    recommendations: list[str]

# Категории персональных данных по закону РК
PERSONAL_DATA_CATEGORIES = {
    "special": [
        "ИИН", "Паспорт РК", "IBAN счёт", "Номер банковской карты"
    ],
    "general": [
        "Email адрес", "Телефон КЗ", "IP адрес (внутренний)"
    ],
    "confidential": [
        "БИН (Казахстан)", "Ключевые слова конфиденциальности",
        "Финансовые ключевые слова"
    ]
}

# Права субъектов персональных данных (ст. 8 Закона РК)
SUBJECT_RIGHTS = [
    "Право знать о факте сбора своих данных",
    "Право на доступ к своим данным",
    "Право на исправление неточных данных",
    "Право на удаление данных",
    "Право на отзыв согласия на обработку"
]

# Требования к DLP системе по законодательству РК
REQUIREMENTS = {
    "employee_notification": {
        "description": "Уведомление сотрудников о мониторинге",
        "law_ref": "Трудовой кодекс РК, ст. 23",
        "mandatory": True,
        "implementation": "Включить в трудовой договор и политику ИБ компании"
    },
    "data_localization": {
        "description": "Хранение данных на серверах в РК",
        "law_ref": "Закон РК о персональных данных, ст. 27",
        "mandatory": True,
        "implementation": "Сервер должен физически находиться в Казахстане"
    },
    "data_minimization": {
        "description": "Сбор только необходимых данных",
        "law_ref": "Закон РК о персональных данных, ст. 7",
        "mandatory": True,
        "implementation": "Хранить сниппеты, а не полный контент сообщений"
    },
    "retention_period": {
        "description": "Срок хранения данных — не более 3 лет",
        "law_ref": "Закон РК о персональных данных, ст. 17",
        "mandatory": True,
        "implementation": "Автоматическое удаление логов старше 3 лет"
    },
    "access_control": {
        "description": "Ограниченный доступ к персональным данным",
        "law_ref": "Приказ МЦРиАП РК №410",
        "mandatory": True,
        "implementation": "Ролевая модель: admin / security_officer / viewer"
    },
    "audit_log": {
        "description": "Журнал доступа к персональным данным",
        "law_ref": "Приказ МЦРиАП РК №410",
        "mandatory": True,
        "implementation": "Логировать каждый запрос к инцидентам и логам"
    },
    "consent": {
        "description": "Согласие сотрудника на мониторинг корпоративных устройств",
        "law_ref": "Закон РК о персональных данных, ст. 8",
        "mandatory": True,
        "implementation": "Подписанное согласие при трудоустройстве"
    }
}

def check_compliance(config: dict) -> ComplianceCheck:
    """Проверяет конфигурацию системы на соответствие требованиям РК."""
    violations = []
    recommendations = []

    if not config.get("employee_notification_signed"):
        violations.append("Отсутствует подписанное уведомление сотрудников о мониторинге (ТК РК ст. 23)")

    if not config.get("server_in_kazakhstan"):
        violations.append("Сервер не находится в Казахстане — нарушение локализации данных (Закон о ПД ст. 27)")

    if config.get("store_full_messages", True):
        violations.append("Хранение полного контента сообщений — нарушение принципа минимизации данных")
        recommendations.append("Хранить только сниппеты (первые 200 символов) и хэш контента")

    retention_days = config.get("retention_days", 0)
    if retention_days > 1095:  # 3 года
        violations.append(f"Срок хранения {retention_days} дней превышает допустимые 3 года")

    if not config.get("access_roles_enabled"):
        recommendations.append("Включите ролевую модель доступа")

    if not config.get("audit_log_enabled"):
        violations.append("Отсутствует журнал доступа к персональным данным (Приказ МЦРиАП №410)")

    return ComplianceCheck(
        compliant=len(violations) == 0,
        violations=violations,
        recommendations=recommendations
    )

def get_requirements_report() -> dict:
    """Возвращает полный отчёт по требованиям РК."""
    return {
        "law_framework": [
            "Закон РК «О персональных данных и их защите» №94-V от 21.05.2013",
            "Трудовой кодекс Республики Казахстан",
            "Закон РК «Об информационной безопасности» 2022",
            "Приказ МЦРиАП РК №410"
        ],
        "requirements": REQUIREMENTS,
        "personal_data_categories": PERSONAL_DATA_CATEGORIES,
        "subject_rights": SUBJECT_RIGHTS
    }
