use serde_json::{Map, Value, json};

use super::definition::{QualificationAgent, Question};

const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

pub fn contract_schema(agent: &QualificationAgent) -> Value {
    let properties: Map<String, Value> = agent
        .questions
        .iter()
        .map(|question| (question.id.clone(), question_schema(question)))
        .collect();
    let required: Vec<&str> = agent
        .questions
        .iter()
        .map(|question| question.id.as_str())
        .collect();
    json!({
        "$schema": DIALECT,
        "title": format!("Answers of {}", agent.name),
        "type": "object",
        "properties": {
            "answers": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            }
        },
        "required": ["answers"],
        "additionalProperties": false
    })
}

fn probability() -> Value {
    json!({"type": "number", "minimum": 0, "maximum": 1})
}

fn question_schema(question: &Question) -> Value {
    let values: Vec<&str> = question
        .options
        .iter()
        .map(|option| option.value.as_str())
        .collect();
    let probabilities: Map<String, Value> = values
        .iter()
        .map(|value| ((*value).to_owned(), probability()))
        .collect();
    json!({
        "description": question.wording,
        "type": "object",
        "properties": {
            "type": {"type": "string", "const": "choice"},
            "choice": {"type": "string", "enum": values},
            "confidence": probability(),
            "probabilities": {
                "type": "object",
                "properties": probabilities,
                "required": values,
                "additionalProperties": false
            }
        },
        "required": ["type", "choice", "confidence", "probabilities"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::ValidatedProfile;

    fn agent() -> QualificationAgent {
        let value = json!({
            "schema_version": 3,
            "identity": {"offer": "o", "target_customer": "t"},
            "agents": [{"id": "ats", "name": "ATS", "source": {"kind": "ats", "keywords": ["sso"]}}],
            "scoring": {"factors": [{"signal": "a", "weight": 1}]},
            "contact_ranking": {"preferred_local_parts": []},
            "llm": {"qualification_threshold": 50, "relevance_criteria": "c"}
        });
        let mut definition = ValidatedProfile::parse(value).unwrap().definition().clone();
        let mut agent = definition.qualification_agents.remove(0);
        agent.questions[0].id = "q-one".to_owned();
        agent
    }

    #[test]
    fn schema_describes_every_question_with_its_options() {
        let schema = contract_schema(&agent());
        let answers = &schema["properties"]["answers"];
        assert_eq!(schema["required"], json!(["answers"]));
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(answers["required"], json!(["q-one"]));
        assert_eq!(answers["additionalProperties"], json!(false));
        let question = &answers["properties"]["q-one"];
        assert_eq!(question["properties"]["type"]["const"], "choice");
        assert_eq!(
            question["properties"]["choice"]["enum"],
            json!(["forte", "moyenne", "faible"])
        );
        assert_eq!(
            question["required"],
            json!(["type", "choice", "confidence", "probabilities"])
        );
        assert_eq!(question["additionalProperties"], json!(false));
        assert_eq!(
            question["description"],
            "Ce lead est-il pertinent pour l'offre ?"
        );
    }

    #[test]
    fn confidence_and_probabilities_are_bounded_numbers() {
        let schema = contract_schema(&agent());
        let question = &schema["properties"]["answers"]["properties"]["q-one"];
        let bounded = json!({"type": "number", "minimum": 0, "maximum": 1});
        assert_eq!(question["properties"]["confidence"], bounded);
        let probabilities = &question["properties"]["probabilities"];
        assert_eq!(
            probabilities["required"],
            json!(["forte", "moyenne", "faible"])
        );
        assert_eq!(probabilities["additionalProperties"], json!(false));
        for key in ["forte", "moyenne", "faible"] {
            assert_eq!(probabilities["properties"][key], bounded);
        }
        assert_eq!(probabilities["properties"].as_object().unwrap().len(), 3);
    }

    #[test]
    fn schema_covers_several_questions_independently() {
        let mut agent = agent();
        let mut second = agent.questions[0].clone();
        second.id = "second".to_owned();
        second.options.truncate(2);
        agent.questions.push(second);
        let schema = contract_schema(&agent);
        let answers = &schema["properties"]["answers"];
        assert_eq!(answers["required"], json!(["q-one", "second"]));
        assert_eq!(
            answers["properties"]["second"]["properties"]["choice"]["enum"],
            json!(["forte", "moyenne"])
        );
        assert_eq!(
            answers["properties"]["q-one"]["properties"]["choice"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }
}
