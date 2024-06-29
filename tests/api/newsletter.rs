use uuid::Uuid;
use wiremock::{
    matchers::{any, method, path},
    Mock, ResponseTemplate,
};

use crate::helper::{spawn_app, ConfirmationLinks, TestApp};

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;
    // To Assert No req fired to email service
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&app.email_server)
        .await;

    let newsletter_req_body = serde_json::json!({
        "title": "Newsletter Title",
        "content": {
            "text": "Newsletter in plain text",
            "html": "<h1>Newsletter</h1> as HTML"
        }
    });

    let response = app.post_newsletter(newsletter_req_body).await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscriber() {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    let newsletter_req_body = serde_json::json!({
        "title": "Newsletter Title",
        "content": {
            "text": "Newsletter in plain text",
            "html": "<h1>Newsletter</h1> as HTML"
        }
    });

    let response = app.post_newsletter(newsletter_req_body).await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn newsletters_returns_400_for_invalid_body() {
    let app = spawn_app().await;
    let test_case = [
        (
            serde_json::json!({
                "content": {
                    "text": "newsletter plain text",
                    "html": "<h1>newsletter</h1> html"
                }
            }),
            "missing title",
        ),
        (
            serde_json::json!({
                "title": "test title"
            }),
            "missing content",
        ),
    ];

    for (invalid_body, msg) in test_case {
        let res = app.post_newsletter(invalid_body).await;

        assert_eq!(
            res.status().as_u16(),
            400,
            "The API did not fail with status 400 when payload was {msg}"
        );
    }
}

async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=monkey&email=monk%40gmail.com";

    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;
    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();
    let email_req = &app.email_server.received_requests().await.unwrap()[0];
    app.get_confirmation_link(email_req)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    let confirmation_link = create_unconfirmed_subscriber(app).await;
    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[tokio::test]
async fn requests_missing_authroization_are_rejected() {
    let app = spawn_app().await;
    let response = reqwest::Client::new()
        .post(format!("{}/newsletters", app.address))
        .json(&serde_json::json!({
            "title": "sample title",
            "content": {
                "text": "Newsletter",
                "html": "<h1>Newsletter</h1>"
            }
        }))
        .send()
        .await
        .expect("Failed sending request to newsletter endpoint.");

    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(
        r#"Basic realm="publish""#,
        response.headers()["WWW-Authenticate"]
    );
}

#[tokio::test]
async fn non_existing_user_is_rejected() {
    let app = spawn_app().await;
    let (username, password) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());

    let response = reqwest::Client::new()
        .post(format!("{}/newsletters", &app.address))
        .basic_auth(username, Some(password))
        .json(&serde_json::json!({
            "title": "Newsletter title",
            "content": {
                "text": "Newsletter plain text",
                "html": "<h1>Newsletter</h1> plain text"
            }
        }))
        .send()
        .await
        .expect("Failed sending request");
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(
        r#"Basic realm="publish""#,
        response.headers()["WWW-Authenticate"]
    )
}

#[tokio::test]
async fn invalid_password_is_rejected() {
    let app = spawn_app().await;
    let (username, password) = (&app.test_user.username, Uuid::new_v4().to_string());
    assert_ne!(app.test_user.password, password);
    let response = reqwest::Client::new()
        .post(format!("{}/newsletters", &app.address))
        .basic_auth(username, Some(password))
        .json(&serde_json::json!({
            "title": "Newsletter title",
            "content": {
                "text": "Newsletter plain text",
                "html": "<h1>Newsletter</h1> plain text"
            }
        }))
        .send()
        .await
        .expect("Failed sending request");
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(
        r#"Basic realm="publish""#,
        response.headers()["WWW-Authenticate"]
    )
}