//! O catálogo de conectores: o endereço oficial da API de cada serviço e como a chave vai
//! no pedido. Para acrescentar um serviço, basta uma linha aqui (o teste confere o resto).

use super::Auth::{self, Basic, Bearer, Caminho, Consulta, ConsultaPar, Nenhuma};
use super::Cat::*;
use super::Conector as C;

/// Cabeçalho com a chave pura.
const fn h(nome: &'static str) -> Auth {
    Auth::Cabecalho { nome, prefixo: "" }
}

/// Cabeçalho com prefixo.
const fn hp(nome: &'static str, prefixo: &'static str) -> Auth {
    Auth::Cabecalho { nome, prefixo }
}

/// Duas partes em dois cabeçalhos.
const fn par(a: &'static str, b: &'static str, prefixo_b: &'static str) -> Auth {
    Auth::Par { a, b, prefixo_b }
}

const GITHUB_EXTRAS: &[(&str, &str)] =
    &[("Accept", "application/vnd.github+json"), ("X-GitHub-Api-Version", "2022-11-28")];
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
const TOKEN_GOOGLE: &str = "token de acesso OAuth do Google (ex.: gerado no OAuth Playground com os escopos do \
serviço; expira em 1 h). A mesma chave vale para Gmail, Drive, Agenda, Planilhas, YouTube...";
const TOKEN_MS: &str = "token de acesso do Microsoft Graph (ex.: do Graph Explorer; expira). A mesma chave vale \
para Outlook, OneDrive, Teams, To Do e SharePoint.";

/// Todos os conectores.
pub static CATALOGO: &[C] = &[
    // ------------------------------------------------------------------ código
    C::novo("github", "GitHub", Codigo, "https://api.github.com", Bearer, Some("/user"))
        .extras(GITHUB_EXTRAS)
        .dica("token pessoal em github.com/settings/tokens"),
    C::novo("gitlab", "GitLab", Codigo, "https://gitlab.com/api/v4", Bearer, Some("/user")),
    C::novo("bitbucket", "Bitbucket", Codigo, "https://api.bitbucket.org/2.0", Basic, Some("/user"))
        .dica("email:token de API do Atlassian"),
    C::novo("gitea", "Gitea / Forgejo", Codigo, "https://{conta}/api/v1", hp("Authorization", "token "), Some("/user"))
        .dica("conta = seu servidor (ex.: git.meusite.com); chave = token de acesso"),
    C::novo(
        "azure-devops",
        "Azure DevOps",
        Codigo,
        "https://dev.azure.com/{conta}",
        Basic,
        Some("/_apis/projects?api-version=7.1"),
    )
    .dica("conta = sua organização; chave = :SEU_PAT (dois-pontos antes do token)"),
    C::novo("npm", "npm", Codigo, "https://registry.npmjs.org", Bearer, Some("/-/whoami")),
    C::novo("crates-io", "crates.io", Codigo, "https://crates.io/api/v1", Nenhuma, Some("/crates?q=tokio&per_page=1")),
    C::novo(
        "stackexchange",
        "Stack Overflow",
        Codigo,
        "https://api.stackexchange.com/2.3",
        Nenhuma,
        Some("/info?site=stackoverflow"),
    ),
    C::novo("circleci", "CircleCI", Codigo, "https://circleci.com/api/v2", h("Circle-Token"), Some("/me")),
    C::novo("sentry", "Sentry", Codigo, "https://sentry.io/api/0", Bearer, Some("/organizations/")),
    C::novo("snyk", "Snyk", Codigo, "https://api.snyk.io/v1", hp("Authorization", "token "), Some("/user/me")),
    C::novo("postman", "Postman", Codigo, "https://api.getpostman.com", h("X-Api-Key"), Some("/me")),
    C::novo("gitbook", "GitBook", Codigo, "https://api.gitbook.com/v1", Bearer, Some("/user")),
    C::novo("jira", "Jira", Codigo, "https://{conta}/rest/api/3", Basic, Some("/myself"))
        .dica("conta = suaempresa.atlassian.net; chave = email:token de API"),
    C::novo("confluence", "Confluence", Codigo, "https://{conta}/wiki/rest/api", Basic, Some("/user/current"))
        .dica("conta = suaempresa.atlassian.net; chave = email:token de API"),
    C::novo("linear", "Linear", Codigo, "https://api.linear.app", h("Authorization"), None)
        .dica("chave de API pessoal; use POST /graphql com {query}"),
    C::novo(
        "launchdarkly",
        "LaunchDarkly",
        Codigo,
        "https://app.launchdarkly.com/api/v2",
        h("Authorization"),
        Some("/projects"),
    ),
    C::novo("posthog", "PostHog", Codigo, "https://us.posthog.com/api", Bearer, Some("/users/@me/")),
    C::novo(
        "terraform-cloud",
        "HCP Terraform",
        Codigo,
        "https://app.terraform.io/api/v2",
        Bearer,
        Some("/account/details"),
    ),
    C::novo("pulumi", "Pulumi", Codigo, "https://api.pulumi.com/api", hp("Authorization", "token "), Some("/user")),
    C::novo("devto", "DEV Community", Codigo, "https://dev.to/api", h("api-key"), Some("/users/me")),
    C::novo("hashnode", "Hashnode", Codigo, "https://gql.hashnode.com", h("Authorization"), None)
        .dica("token pessoal; POST / com {query} (GraphQL)"),
    C::novo("huggingface", "Hugging Face", Codigo, "https://huggingface.co/api", Bearer, Some("/whoami-v2")),
    C::novo("replicate", "Replicate", Codigo, "https://api.replicate.com/v1", Bearer, Some("/account")),
    // ------------------------------------------------------------------ nuvem
    C::novo("vercel", "Vercel", Nuvem, "https://api.vercel.com", Bearer, Some("/v2/user")),
    C::novo("netlify", "Netlify", Nuvem, "https://api.netlify.com/api/v1", Bearer, Some("/user")),
    C::novo("render", "Render", Nuvem, "https://api.render.com/v1", Bearer, Some("/services?limit=1")),
    C::novo("heroku", "Heroku", Nuvem, "https://api.heroku.com", Bearer, Some("/account"))
        .extras(&[("Accept", "application/vnd.heroku+json; version=3")]),
    C::novo("digitalocean", "DigitalOcean", Nuvem, "https://api.digitalocean.com/v2", Bearer, Some("/account")),
    C::novo("linode", "Linode (Akamai)", Nuvem, "https://api.linode.com/v4", Bearer, Some("/profile")),
    C::novo("vultr", "Vultr", Nuvem, "https://api.vultr.com/v2", Bearer, Some("/account")),
    C::novo("hetzner", "Hetzner Cloud", Nuvem, "https://api.hetzner.cloud/v1", Bearer, Some("/servers")),
    C::novo(
        "cloudflare",
        "Cloudflare",
        Nuvem,
        "https://api.cloudflare.com/client/v4",
        Bearer,
        Some("/user/tokens/verify"),
    ),
    C::novo("fly", "Fly.io", Nuvem, "https://api.machines.dev/v1", Bearer, None),
    C::novo("supabase", "Supabase", Nuvem, "https://api.supabase.com/v1", Bearer, Some("/projects")),
    C::novo("neon", "Neon", Nuvem, "https://console.neon.tech/api/v2", Bearer, Some("/projects")),
    C::novo("turso", "Turso", Nuvem, "https://api.turso.tech/v1", Bearer, Some("/organizations")),
    C::novo("upstash-redis", "Upstash Redis", Nuvem, "https://{conta}", Bearer, None)
        .dica("conta = o endereço REST do banco (ex.: xxx.upstash.io); chave = o token REST"),
    C::novo(
        "hostinger",
        "Hostinger",
        Nuvem,
        "https://developers.hostinger.com/api",
        Bearer,
        Some("/vps/v1/virtual-machines"),
    )
    .dica("token de API no hPanel (Perfil → API)"),
    C::novo(
        "godaddy",
        "GoDaddy",
        Nuvem,
        "https://api.godaddy.com/v1",
        hp("Authorization", "sso-key "),
        Some("/domains"),
    )
    .dica("chave = KEY:SECRET"),
    C::novo("cpanel", "cPanel", Nuvem, "https://{conta}", hp("Authorization", "cpanel "), None)
        .dica("conta = seusite.com.br:2083; chave = usuario:TOKEN; caminho /execute/Modulo/funcao"),
    C::novo("portainer", "Portainer", Nuvem, "https://{conta}/api", h("X-API-Key"), Some("/endpoints"))
        .dica("conta = seu Portainer (ex.: painel.meusite.com:9443)"),
    C::novo(
        "proxmox",
        "Proxmox VE",
        Nuvem,
        "https://{conta}/api2/json",
        hp("Authorization", "PVEAPIToken="),
        Some("/version"),
    )
    .dica("conta = servidor:8006; chave = usuario@pam!idtoken=segredo"),
    C::novo("coolify", "Coolify", Nuvem, "https://{conta}/api/v1", Bearer, Some("/version"))
        .dica("conta = o endereço do seu Coolify"),
    C::novo("smartthings", "SmartThings", Automacao, "https://api.smartthings.com/v1", Bearer, Some("/devices")),
    C::novo("home-assistant", "Home Assistant", Automacao, "https://{conta}/api", Bearer, Some("/"))
        .dica("conta = o endereço do seu Home Assistant; chave = token de longa duração (Perfil → Segurança)"),
    // ------------------------------------------------------------------ monitoramento
    C::novo(
        "datadog",
        "Datadog",
        Monitoramento,
        "https://api.datadoghq.com/api/v1",
        par("DD-API-KEY", "DD-APPLICATION-KEY", ""),
        Some("/validate"),
    )
    .dica("chave = API_KEY:APPLICATION_KEY"),
    C::novo("new-relic", "New Relic", Monitoramento, "https://api.newrelic.com", h("API-Key"), None)
        .dica("chave de usuário; POST /graphql com {query}"),
    C::novo("grafana", "Grafana", Monitoramento, "https://{conta}/api", Bearer, Some("/user"))
        .dica("conta = seu Grafana (ex.: meuorg.grafana.net); chave = token de conta de serviço"),
    C::novo(
        "pagerduty",
        "PagerDuty",
        Monitoramento,
        "https://api.pagerduty.com",
        hp("Authorization", "Token token="),
        Some("/abilities"),
    ),
    C::novo(
        "opsgenie",
        "Opsgenie",
        Monitoramento,
        "https://api.opsgenie.com/v2",
        hp("Authorization", "GenieKey "),
        Some("/account"),
    ),
    C::novo(
        "statuspage",
        "Statuspage",
        Monitoramento,
        "https://api.statuspage.io/v1",
        hp("Authorization", "OAuth "),
        Some("/pages"),
    ),
    C::novo(
        "betterstack",
        "Better Stack Uptime",
        Monitoramento,
        "https://uptime.betterstack.com/api/v2",
        Bearer,
        Some("/monitors"),
    ),
    C::novo("plausible", "Plausible", Monitoramento, "https://plausible.io/api/v1", Bearer, None),
    C::novo("umami", "Umami", Monitoramento, "https://{conta}/api", Bearer, None)
        .dica("conta = o endereço do seu Umami"),
    C::novo("matomo", "Matomo", Monitoramento, "https://{conta}", Consulta("token_auth"), None)
        .dica("conta = o endereço do seu Matomo; caminho /index.php?module=API&method=..."),
    C::novo("google", "Google (conta)", Arquivos, "https://www.googleapis.com", Bearer, Some("/oauth2/v3/userinfo"))
        .dica(TOKEN_GOOGLE),
    C::novo(
        "google-analytics",
        "Google Analytics",
        Monitoramento,
        "https://analyticsdata.googleapis.com/v1beta",
        Bearer,
        None,
    )
    .chave("google"),
    C::novo(
        "search-console",
        "Google Search Console",
        Monitoramento,
        "https://www.googleapis.com/webmasters/v3",
        Bearer,
        Some("/sites"),
    )
    .chave("google"),
    C::novo(
        "google-tag-manager",
        "Google Tag Manager",
        Monitoramento,
        "https://tagmanager.googleapis.com/tagmanager/v2",
        Bearer,
        Some("/accounts"),
    )
    .chave("google"),
    // ------------------------------------------------------------------ projetos
    C::novo("notion", "Notion", Projetos, "https://api.notion.com/v1", Bearer, Some("/users/me"))
        .extras(&[("Notion-Version", "2022-06-28")])
        .dica("token da integração em notion.so/my-integrations (compartilhe as páginas com ela)"),
    C::novo("asana", "Asana", Projetos, "https://app.asana.com/api/1.0", Bearer, Some("/users/me")),
    C::novo("trello", "Trello", Projetos, "https://api.trello.com/1", ConsultaPar("key", "token"), Some("/members/me"))
        .dica("chave = APIKEY:TOKEN (trello.com/power-ups/admin)"),
    C::novo("clickup", "ClickUp", Projetos, "https://api.clickup.com/api/v2", h("Authorization"), Some("/user")),
    C::novo("monday", "monday.com", Projetos, "https://api.monday.com/v2", h("Authorization"), None)
        .dica("token pessoal; POST / com {query} (GraphQL)"),
    C::novo("airtable", "Airtable", Projetos, "https://api.airtable.com/v0", Bearer, Some("/meta/whoami")),
    C::novo("todoist", "Todoist", Projetos, "https://api.todoist.com/api/v1", Bearer, None),
    C::novo("toggl", "Toggl Track", Projetos, "https://api.track.toggl.com/api/v9", Basic, Some("/me"))
        .dica("chave = SEU_TOKEN:api_token"),
    C::novo("clockify", "Clockify", Projetos, "https://api.clockify.me/api/v1", h("X-Api-Key"), Some("/user")),
    C::novo(
        "harvest",
        "Harvest",
        Projetos,
        "https://api.harvestapp.com/v2",
        par("Harvest-Account-Id", "Authorization", "Bearer "),
        Some("/users/me"),
    )
    .dica("chave = ACCOUNT_ID:TOKEN"),
    C::novo("coda", "Coda", Projetos, "https://coda.io/apis/v1", Bearer, Some("/whoami")),
    C::novo("smartsheet", "Smartsheet", Projetos, "https://api.smartsheet.com/2.0", Bearer, Some("/users/me")),
    C::novo("miro", "Miro", Projetos, "https://api.miro.com/v2", Bearer, Some("/boards?limit=1")),
    C::novo("figma", "Figma", Projetos, "https://api.figma.com/v1", h("X-Figma-Token"), Some("/me")),
    C::novo("canva", "Canva", Projetos, "https://api.canva.com/rest/v1", Bearer, Some("/users/me")),
    C::novo("calendly", "Calendly", Projetos, "https://api.calendly.com", Bearer, Some("/users/me")),
    C::novo("cal-com", "Cal.com", Projetos, "https://api.cal.com/v2", Bearer, Some("/me")),
    C::novo("typeform", "Typeform", Projetos, "https://api.typeform.com", Bearer, Some("/me")),
    C::novo("jotform", "Jotform", Projetos, "https://api.jotform.com", h("APIKEY"), Some("/user")),
    C::novo("surveymonkey", "SurveyMonkey", Projetos, "https://api.surveymonkey.com/v3", Bearer, Some("/users/me")),
    C::novo("google-forms", "Google Forms", Projetos, "https://forms.googleapis.com/v1", Bearer, None).chave("google"),
    C::novo("eventbrite", "Eventbrite", Projetos, "https://www.eventbriteapi.com/v3", Bearer, Some("/users/me/")),
    C::novo("raindrop", "Raindrop.io", Projetos, "https://api.raindrop.io/rest/v1", Bearer, Some("/user")),
    C::novo(
        "readwise",
        "Readwise",
        Projetos,
        "https://readwise.io/api/v2",
        hp("Authorization", "Token "),
        Some("/auth/"),
    ),
    C::novo("feedly", "Feedly", Projetos, "https://cloud.feedly.com/v3", Bearer, Some("/profile")),
    C::novo(
        "pandadoc",
        "PandaDoc",
        Projetos,
        "https://api.pandadoc.com/public/v1",
        hp("Authorization", "API-Key "),
        Some("/members/current/"),
    ),
    C::novo("zapsign", "ZapSign", Projetos, "https://api.zapsign.com.br/api/v1", Bearer, Some("/docs/")),
    C::novo("autentique", "Autentique", Projetos, "https://api.autentique.com.br/v2", Bearer, None)
        .dica("token da API; POST /graphql com {query}"),
    C::novo(
        "google-tasks",
        "Google Tarefas",
        Projetos,
        "https://tasks.googleapis.com/tasks/v1",
        Bearer,
        Some("/users/@me/lists"),
    )
    .chave("google"),
    C::novo("google-docs", "Google Docs", Projetos, "https://docs.googleapis.com/v1", Bearer, None).chave("google"),
    C::novo("google-sheets", "Google Planilhas", Projetos, "https://sheets.googleapis.com/v4", Bearer, None)
        .chave("google"),
    C::novo("google-slides", "Google Apresentações", Projetos, "https://slides.googleapis.com/v1", Bearer, None)
        .chave("google"),
    C::novo("microsoft", "Microsoft 365 (Graph)", Projetos, GRAPH, Bearer, Some("/me")).dica(TOKEN_MS),
    C::novo("microsoft-todo", "Microsoft To Do", Projetos, GRAPH, Bearer, Some("/me/todo/lists")).chave("microsoft"),
    C::novo("onenote", "OneNote", Projetos, GRAPH, Bearer, Some("/me/onenote/notebooks")).chave("microsoft"),
    // ------------------------------------------------------------------ arquivos
    C::novo(
        "google-drive",
        "Google Drive",
        Arquivos,
        "https://www.googleapis.com/drive/v3",
        Bearer,
        Some("/about?fields=user"),
    )
    .chave("google"),
    C::novo("dropbox", "Dropbox", Arquivos, "https://api.dropboxapi.com/2", Bearer, None)
        .dica("token do app em dropbox.com/developers; POST /users/get_current_account"),
    C::novo("box", "Box", Arquivos, "https://api.box.com/2.0", Bearer, Some("/users/me")),
    C::novo("onedrive", "OneDrive", Arquivos, GRAPH, Bearer, Some("/me/drive")).chave("microsoft"),
    C::novo("sharepoint", "SharePoint", Arquivos, GRAPH, Bearer, Some("/sites?search=*")).chave("microsoft"),
    C::novo("cloudinary", "Cloudinary", Arquivos, "https://api.cloudinary.com/v1_1/{conta}", Basic, Some("/usage"))
        .dica("conta = cloud name; chave = API_KEY:API_SECRET"),
    C::novo("imgur", "Imgur", Arquivos, "https://api.imgur.com/3", hp("Authorization", "Client-ID "), Some("/credits")),
    // ------------------------------------------------------------------ e-mail e agenda
    C::novo("gmail", "Gmail", Email, "https://gmail.googleapis.com/gmail/v1", Bearer, Some("/users/me/profile"))
        .chave("google"),
    C::novo(
        "google-calendar",
        "Google Agenda",
        Email,
        "https://www.googleapis.com/calendar/v3",
        Bearer,
        Some("/users/me/calendarList"),
    )
    .chave("google"),
    C::novo(
        "google-contacts",
        "Google Contatos",
        Email,
        "https://people.googleapis.com/v1",
        Bearer,
        Some("/people/me?personFields=names"),
    )
    .chave("google"),
    C::novo("outlook", "Outlook (e-mail)", Email, GRAPH, Bearer, Some("/me/messages?$top=1")).chave("microsoft"),
    C::novo("outlook-calendar", "Outlook (agenda)", Email, GRAPH, Bearer, Some("/me/events?$top=1")).chave("microsoft"),
    C::novo("teams", "Microsoft Teams", Mensagens, GRAPH, Bearer, Some("/me/joinedTeams")).chave("microsoft"),
    C::novo("zoom", "Zoom", Email, "https://api.zoom.us/v2", Bearer, Some("/users/me")),
    C::novo("webex", "Webex", Email, "https://webexapis.com/v1", Bearer, Some("/people/me")),
    C::novo("sendgrid", "SendGrid", Email, "https://api.sendgrid.com/v3", Bearer, Some("/user/profile")),
    C::novo("mailgun", "Mailgun", Email, "https://api.mailgun.net/v3", Basic, Some("/domains"))
        .dica("chave = api:SUA_CHAVE"),
    C::novo(
        "postmark",
        "Postmark",
        Email,
        "https://api.postmarkapp.com",
        h("X-Postmark-Server-Token"),
        Some("/server"),
    ),
    C::novo("resend", "Resend", Email, "https://api.resend.com", Bearer, Some("/domains")),
    C::novo("brevo", "Brevo", Email, "https://api.brevo.com/v3", h("api-key"), Some("/account")),
    C::novo("mailchimp", "Mailchimp", Email, "https://{conta}.api.mailchimp.com/3.0", Basic, Some("/ping"))
        .dica("conta = o datacenter no fim da chave (ex.: us21); chave = qualquer:SUA_CHAVE"),
    C::novo(
        "mailerlite",
        "MailerLite",
        Email,
        "https://connect.mailerlite.com/api",
        Bearer,
        Some("/subscribers?limit=1"),
    ),
    C::novo("kit", "Kit (ConvertKit)", Email, "https://api.kit.com/v4", h("X-Kit-Api-Key"), Some("/account")),
    C::novo(
        "klaviyo",
        "Klaviyo",
        Email,
        "https://a.klaviyo.com/api",
        hp("Authorization", "Klaviyo-API-Key "),
        Some("/accounts/"),
    )
    .extras(&[("revision", "2024-10-15")]),
    C::novo("activecampaign", "ActiveCampaign", Email, "https://{conta}/api/3", h("Api-Token"), Some("/users/me"))
        .dica("conta = suaconta.api-us1.com"),
    // ------------------------------------------------------------------ mensagens e atendimento
    C::novo("telegram", "Telegram (bot)", Mensagens, "https://api.telegram.org/bot{chave}", Caminho, Some("/getMe"))
        .dica("token do bot dado pelo @BotFather"),
    C::novo(
        "discord",
        "Discord (bot)",
        Mensagens,
        "https://discord.com/api/v10",
        hp("Authorization", "Bot "),
        Some("/users/@me"),
    ),
    C::novo("slack", "Slack", Mensagens, "https://slack.com/api", Bearer, Some("/auth.test")),
    C::novo("whatsapp", "WhatsApp Cloud API", Mensagens, "https://graph.facebook.com/v21.0", Bearer, None)
        .dica("token da Meta (WhatsApp → Configuração da API); caminho /{phone_number_id}/messages"),
    C::novo(
        "evolution-api",
        "Evolution API (WhatsApp)",
        Mensagens,
        "https://{conta}",
        h("apikey"),
        Some("/instance/fetchInstances"),
    )
    .dica("conta = o endereço da sua Evolution API; chave = a apikey global"),
    C::novo("chatwoot", "Chatwoot", Mensagens, "https://{conta}/api/v1", h("api_access_token"), Some("/profile"))
        .dica("conta = app.chatwoot.com ou o seu servidor"),
    C::novo(
        "twilio",
        "Twilio (SMS e voz)",
        Mensagens,
        "https://api.twilio.com/2010-04-01",
        Basic,
        Some("/Accounts.json"),
    )
    .dica("chave = ACCOUNT_SID:AUTH_TOKEN"),
    C::novo("manychat", "ManyChat", Mensagens, "https://api.manychat.com", Bearer, Some("/fb/page/getInfo")),
    C::novo("intercom", "Intercom", Mensagens, "https://api.intercom.io", Bearer, Some("/me")),
    C::novo("front", "Front", Mensagens, "https://api2.frontapp.com", Bearer, Some("/me")),
    C::novo("zendesk", "Zendesk", Mensagens, "https://{conta}/api/v2", Basic, Some("/users/me"))
        .dica("conta = suaempresa.zendesk.com; chave = email/token:SEU_TOKEN"),
    C::novo("freshdesk", "Freshdesk", Mensagens, "https://{conta}/api/v2", Basic, Some("/agents/me"))
        .dica("conta = suaempresa.freshdesk.com; chave = SUA_CHAVE:X"),
    C::novo("ntfy", "ntfy (notificações)", Mensagens, "https://ntfy.sh", Nenhuma, None)
        .dica("público: POST /seu-topico com o texto no corpo, e o celular com o app ntfy recebe"),
    C::novo("gotify", "Gotify", Mensagens, "https://{conta}", h("X-Gotify-Key"), None)
        .dica("conta = o endereço do seu Gotify; chave = token do app"),
    C::novo("matrix", "Matrix", Mensagens, "https://{conta}/_matrix/client/v3", Bearer, Some("/account/whoami"))
        .dica("conta = seu servidor (ex.: matrix.org)"),
    // ------------------------------------------------------------------ redes sociais
    C::novo("x", "X (Twitter)", Social, "https://api.x.com/2", Bearer, Some("/users/me"))
        .dica("token de usuário OAuth 2.0 do portal developer.x.com (para postar: escopos tweet.write e users.read)"),
    C::novo("linkedin", "LinkedIn", Social, "https://api.linkedin.com/v2", Bearer, Some("/userinfo")),
    C::novo("meta", "Facebook (Graph)", Social, "https://graph.facebook.com/v21.0", Bearer, Some("/me")),
    C::novo(
        "instagram",
        "Instagram",
        Social,
        "https://graph.instagram.com/v21.0",
        Consulta("access_token"),
        Some("/me?fields=id,username"),
    ),
    C::novo(
        "threads",
        "Threads",
        Social,
        "https://graph.threads.net/v1.0",
        Consulta("access_token"),
        Some("/me?fields=id,username"),
    ),
    C::novo("reddit", "Reddit", Social, "https://oauth.reddit.com", Bearer, Some("/api/v1/me")),
    C::novo(
        "youtube",
        "YouTube",
        Social,
        "https://www.googleapis.com/youtube/v3",
        Bearer,
        Some("/channels?part=snippet&mine=true"),
    )
    .chave("google"),
    C::novo(
        "tiktok",
        "TikTok",
        Social,
        "https://open.tiktokapis.com/v2",
        Bearer,
        Some("/user/info/?fields=open_id,display_name"),
    ),
    C::novo("pinterest", "Pinterest", Social, "https://api.pinterest.com/v5", Bearer, Some("/user_account")),
    C::novo(
        "twitch",
        "Twitch",
        Social,
        "https://api.twitch.tv/helix",
        par("Client-Id", "Authorization", "Bearer "),
        Some("/users"),
    )
    .dica("chave = CLIENT_ID:TOKEN"),
    C::novo("vimeo", "Vimeo", Social, "https://api.vimeo.com", Bearer, Some("/me")),
    C::novo("mastodon", "Mastodon", Social, "https://{conta}/api/v1", Bearer, Some("/accounts/verify_credentials"))
        .dica("conta = sua instância (ex.: mastodon.social)"),
    C::novo("bluesky", "Bluesky", Social, "https://bsky.social/xrpc", Bearer, Some("/com.atproto.server.getSession"))
        .dica("token de sessão (accessJwt de com.atproto.server.createSession)"),
    C::novo("producthunt", "Product Hunt", Social, "https://api.producthunt.com/v2/api", Bearer, None)
        .dica("token de desenvolvedor; POST /graphql com {query}"),
    C::novo("dribbble", "Dribbble", Social, "https://api.dribbble.com/v2", Bearer, Some("/user")),
    C::novo(
        "lastfm",
        "Last.fm",
        Social,
        "https://ws.audioscrobbler.com/2.0",
        Consulta("api_key"),
        Some("/?method=chart.gettopartists&format=json&limit=1"),
    ),
    C::novo("spotify", "Spotify", Midia, "https://api.spotify.com/v1", Bearer, Some("/me")),
    // ------------------------------------------------------------------ sites e conteúdo
    C::novo("wordpress", "WordPress (seu site)", Conteudo, "https://{conta}/wp-json/wp/v2", Basic, Some("/users/me"))
        .dica("conta = seusite.com.br; chave = usuario:senha-de-aplicativo (sem os espaços)"),
    C::novo(
        "wordpress-com",
        "WordPress.com",
        Conteudo,
        "https://public-api.wordpress.com/rest/v1.1",
        Bearer,
        Some("/me"),
    ),
    C::novo(
        "ghost",
        "Ghost (leitura)",
        Conteudo,
        "https://{conta}/ghost/api/content",
        Consulta("key"),
        Some("/posts/?limit=1"),
    )
    .dica("conta = seu blog Ghost; chave = Content API key"),
    C::novo("webflow", "Webflow", Conteudo, "https://api.webflow.com/v2", Bearer, Some("/token/authorized_by")),
    C::novo(
        "squarespace",
        "Squarespace",
        Conteudo,
        "https://api.squarespace.com/1.0",
        Bearer,
        Some("/authorization/website"),
    ),
    C::novo("contentful", "Contentful", Conteudo, "https://cdn.contentful.com", Bearer, None),
    C::novo("sanity", "Sanity", Conteudo, "https://{conta}.api.sanity.io/v2021-10-21", Bearer, None)
        .dica("conta = o id do projeto"),
    C::novo("strapi", "Strapi", Conteudo, "https://{conta}/api", Bearer, None).dica("conta = o endereço do seu Strapi"),
    C::novo("bitly", "Bitly", Conteudo, "https://api-ssl.bitly.com/v4", Bearer, Some("/user")),
    C::novo("shortio", "Short.io", Conteudo, "https://api.short.io", h("authorization"), Some("/api/domains")),
    C::novo("rebrandly", "Rebrandly", Conteudo, "https://api.rebrandly.com/v1", h("apikey"), Some("/account")),
    C::novo(
        "rd-station",
        "RD Station Marketing",
        Conteudo,
        "https://api.rd.services",
        Bearer,
        Some("/marketing/account_info"),
    ),
    C::novo("hubspot", "HubSpot", Conteudo, "https://api.hubapi.com", Bearer, Some("/crm/v3/objects/contacts?limit=1")),
    C::novo(
        "pipedrive",
        "Pipedrive",
        Conteudo,
        "https://api.pipedrive.com/v1",
        Consulta("api_token"),
        Some("/users/me"),
    ),
    C::novo("salesforce", "Salesforce", Conteudo, "https://{conta}/services/data/v60.0", Bearer, Some("/limits"))
        .dica("conta = suaempresa.my.salesforce.com; chave = token de acesso"),
    C::novo("close", "Close CRM", Conteudo, "https://api.close.com/api/v1", Basic, Some("/me/"))
        .dica("chave = SUA_CHAVE: (com dois-pontos no fim)"),
    // ------------------------------------------------------------------ lojas e vendas
    C::novo(
        "shopify",
        "Shopify",
        Lojas,
        "https://{conta}/admin/api/2024-10",
        h("X-Shopify-Access-Token"),
        Some("/shop.json"),
    )
    .dica("conta = sualoja.myshopify.com; chave = token do app personalizado"),
    C::novo("woocommerce", "WooCommerce", Lojas, "https://{conta}/wp-json/wc/v3", Basic, Some("/system_status"))
        .dica("conta = sualoja.com.br; chave = ck_...:cs_..."),
    C::novo(
        "bigcommerce",
        "BigCommerce",
        Lojas,
        "https://api.bigcommerce.com/stores/{conta}/v3",
        h("X-Auth-Token"),
        Some("/catalog/summary"),
    )
    .dica("conta = o store hash"),
    C::novo(
        "nuvemshop",
        "Nuvemshop",
        Lojas,
        "https://api.nuvemshop.com.br/v1/{conta}",
        hp("Authentication", "bearer "),
        Some("/store"),
    )
    .dica("conta = o id da loja; chave = token de acesso do app"),
    C::novo("mercadolivre", "Mercado Livre", Lojas, "https://api.mercadolibre.com", Bearer, Some("/users/me")),
    C::novo("bling", "Bling", Lojas, "https://api.bling.com.br/Api/v3", Bearer, None),
    C::novo("melhor-envio", "Melhor Envio", Lojas, "https://melhorenvio.com.br/api/v2", Bearer, Some("/me")),
    C::novo("gumroad", "Gumroad", Lojas, "https://api.gumroad.com/v2", Consulta("access_token"), Some("/user")),
    C::novo("lemonsqueezy", "Lemon Squeezy", Lojas, "https://api.lemonsqueezy.com/v1", Bearer, Some("/users/me"))
        .extras(&[("Accept", "application/vnd.api+json")]),
    C::novo("paddle", "Paddle", Lojas, "https://api.paddle.com", Bearer, Some("/products?per_page=1")),
    C::novo(
        "yelp",
        "Yelp",
        Lojas,
        "https://api.yelp.com/v3",
        Bearer,
        Some("/businesses/search?location=Sao+Paulo&limit=1"),
    ),
    // ------------------------------------------------------------------ finanças
    C::novo("mercadopago", "Mercado Pago", Financas, "https://api.mercadopago.com", Bearer, Some("/users/me")),
    C::novo("stripe", "Stripe", Financas, "https://api.stripe.com/v1", Bearer, Some("/balance")),
    C::novo("paypal", "PayPal", Financas, "https://api-m.paypal.com", Bearer, None)
        .dica("token de acesso OAuth (client credentials)"),
    C::novo("pagbank", "PagBank (PagSeguro)", Financas, "https://api.pagseguro.com", Bearer, None),
    C::novo("asaas", "Asaas", Financas, "https://api.asaas.com/v3", h("access_token"), Some("/myAccount")),
    C::novo("pagarme", "Pagar.me", Financas, "https://api.pagar.me/core/v5", Basic, Some("/customers?size=1"))
        .dica("chave = sk_SUA_CHAVE: (com dois-pontos no fim)"),
    C::novo("iugu", "iugu", Financas, "https://api.iugu.com/v1", Basic, None)
        .dica("chave = SEU_TOKEN: (com dois-pontos no fim)"),
    C::novo(
        "coingecko",
        "CoinGecko",
        Financas,
        "https://api.coingecko.com/api/v3",
        h("x-cg-demo-api-key"),
        Some("/ping"),
    ),
    C::novo(
        "alphavantage",
        "Alpha Vantage",
        Financas,
        "https://www.alphavantage.co",
        Consulta("apikey"),
        Some("/query?function=GLOBAL_QUOTE&symbol=IBM"),
    ),
    C::novo("brapi", "brapi (bolsa B3)", Financas, "https://brapi.dev/api", Bearer, Some("/quote/PETR4")),
    C::novo(
        "frankfurter",
        "Frankfurter (câmbio)",
        Financas,
        "https://api.frankfurter.app",
        Nenhuma,
        Some("/latest?from=USD&to=BRL"),
    ),
    C::novo(
        "awesomeapi",
        "AwesomeAPI (cotações)",
        Financas,
        "https://economia.awesomeapi.com.br/json",
        Nenhuma,
        Some("/last/USD-BRL"),
    ),
    // ------------------------------------------------------------------ IA e dados
    C::novo(
        "deepl",
        "DeepL (grátis)",
        Ia,
        "https://api-free.deepl.com/v2",
        hp("Authorization", "DeepL-Auth-Key "),
        Some("/usage"),
    ),
    C::novo(
        "deepl-pro",
        "DeepL Pro",
        Ia,
        "https://api.deepl.com/v2",
        hp("Authorization", "DeepL-Auth-Key "),
        Some("/usage"),
    ),
    C::novo(
        "google-translate",
        "Google Tradutor",
        Ia,
        "https://translation.googleapis.com/language/translate/v2",
        Consulta("key"),
        Some("/languages"),
    ),
    C::novo("elevenlabs", "ElevenLabs", Ia, "https://api.elevenlabs.io/v1", h("xi-api-key"), Some("/user")),
    C::novo(
        "assemblyai",
        "AssemblyAI",
        Ia,
        "https://api.assemblyai.com/v2",
        h("authorization"),
        Some("/transcript?limit=1"),
    ),
    C::novo(
        "deepgram",
        "Deepgram",
        Ia,
        "https://api.deepgram.com/v1",
        hp("Authorization", "Token "),
        Some("/projects"),
    ),
    C::novo("stability", "Stability AI", Ia, "https://api.stability.ai", Bearer, Some("/v1/user/account")),
    C::novo("pinecone", "Pinecone", Ia, "https://api.pinecone.io", h("Api-Key"), Some("/indexes")),
    C::novo("qdrant", "Qdrant", Ia, "https://{conta}", h("api-key"), Some("/collections"))
        .dica("conta = o endereço do cluster (ex.: xxx.cloud.qdrant.io:6333)"),
    C::novo("weaviate", "Weaviate", Ia, "https://{conta}/v1", Bearer, Some("/schema"))
        .dica("conta = o endereço do cluster"),
    C::novo(
        "algolia",
        "Algolia",
        Ia,
        "https://{conta}-dsn.algolia.net/1",
        par("X-Algolia-Application-Id", "X-Algolia-API-Key", ""),
        Some("/indexes"),
    )
    .dica("conta = APP_ID (em minúsculas); chave = APP_ID:API_KEY"),
    C::novo("firecrawl", "Firecrawl", Ia, "https://api.firecrawl.dev/v1", Bearer, None).dica("POST /scrape com {url}"),
    C::novo("apify", "Apify", Ia, "https://api.apify.com/v2", Bearer, Some("/users/me")),
    C::novo(
        "make",
        "Make",
        Automacao,
        "https://{conta}.make.com/api/v2",
        hp("Authorization", "Token "),
        Some("/users/me"),
    )
    .dica("conta = sua zona (us1, us2, eu1, eu2)"),
    C::novo("n8n", "n8n", Automacao, "https://{conta}/api/v1", h("X-N8N-API-KEY"), Some("/workflows?limit=1"))
        .dica("conta = o endereço do seu n8n"),
    C::novo("pipedream", "Pipedream", Automacao, "https://api.pipedream.com/v1", Bearer, Some("/users/me")),
    // ------------------------------------------------------------------ busca, notícias e clima
    C::novo("tavily", "Tavily", Busca, "https://api.tavily.com", Bearer, None).dica("POST /search com {query}"),
    C::novo("exa", "Exa", Busca, "https://api.exa.ai", h("x-api-key"), None).dica("POST /search com {query}"),
    C::novo("serpapi", "SerpApi", Busca, "https://serpapi.com", Consulta("api_key"), Some("/account")),
    C::novo(
        "brave-search",
        "Brave Search",
        Busca,
        "https://api.search.brave.com/res/v1",
        h("X-Subscription-Token"),
        Some("/web/search?q=corvo"),
    ),
    C::novo(
        "wolfram",
        "Wolfram|Alpha",
        Busca,
        "https://api.wolframalpha.com/v1",
        Consulta("appid"),
        Some("/result?i=2%2B2"),
    ),
    C::novo("newsapi", "NewsAPI", Busca, "https://newsapi.org/v2", h("X-Api-Key"), Some("/top-headlines?country=br")),
    C::novo("gnews", "GNews", Busca, "https://gnews.io/api/v4", Consulta("apikey"), Some("/top-headlines?lang=pt")),
    C::novo(
        "guardian",
        "The Guardian",
        Busca,
        "https://content.guardianapis.com",
        Consulta("api-key"),
        Some("/search?q=brazil"),
    ),
    C::novo("nytimes", "The New York Times", Busca, "https://api.nytimes.com/svc", Consulta("api-key"), None),
    C::novo("ipinfo", "IPinfo", Busca, "https://ipinfo.io", Bearer, Some("/json")),
    C::novo(
        "openweather",
        "OpenWeather",
        Busca,
        "https://api.openweathermap.org/data/2.5",
        Consulta("appid"),
        Some("/weather?q=Buriticupu"),
    ),
    C::novo(
        "weatherapi",
        "WeatherAPI",
        Busca,
        "https://api.weatherapi.com/v1",
        Consulta("key"),
        Some("/current.json?q=Buriticupu"),
    ),
    C::novo("tomorrow", "Tomorrow.io", Busca, "https://api.tomorrow.io/v4", Consulta("apikey"), None),
    C::novo(
        "open-meteo",
        "Open-Meteo (clima)",
        Busca,
        "https://api.open-meteo.com/v1",
        Nenhuma,
        Some("/forecast?latitude=-4.32&longitude=-46.45&current_weather=true"),
    ),
    C::novo(
        "hackernews",
        "Hacker News",
        Busca,
        "https://hacker-news.firebaseio.com/v0",
        Nenhuma,
        Some("/topstories.json"),
    ),
    // ------------------------------------------------------------------ mídia
    C::novo(
        "unsplash",
        "Unsplash",
        Midia,
        "https://api.unsplash.com",
        hp("Authorization", "Client-ID "),
        Some("/photos/random"),
    ),
    C::novo("pexels", "Pexels", Midia, "https://api.pexels.com/v1", h("Authorization"), Some("/curated?per_page=1")),
    C::novo("pixabay", "Pixabay", Midia, "https://pixabay.com/api", Consulta("key"), Some("/?q=corvo&per_page=3")),
    C::novo("giphy", "GIPHY", Midia, "https://api.giphy.com/v1", Consulta("api_key"), Some("/gifs/trending?limit=1")),
    C::novo("tmdb", "TMDB (filmes)", Midia, "https://api.themoviedb.org/3", Bearer, Some("/configuration")),
    C::novo("omdb", "OMDb (filmes)", Midia, "https://www.omdbapi.com", Consulta("apikey"), Some("/?t=Matrix")),
    // ------------------------------------------------------------------ mapas
    C::novo(
        "google-maps",
        "Google Maps",
        Mapas,
        "https://maps.googleapis.com/maps/api",
        Consulta("key"),
        Some("/geocode/json?address=Buriticupu"),
    ),
    C::novo("mapbox", "Mapbox", Mapas, "https://api.mapbox.com", Consulta("access_token"), None),
    C::novo(
        "here",
        "HERE",
        Mapas,
        "https://geocode.search.hereapi.com/v1",
        Consulta("apiKey"),
        Some("/geocode?q=Buriticupu"),
    ),
    C::novo(
        "opencage",
        "OpenCage",
        Mapas,
        "https://api.opencagedata.com/geocode/v1",
        Consulta("key"),
        Some("/json?q=Buriticupu"),
    ),
    C::novo("tomtom", "TomTom", Mapas, "https://api.tomtom.com", Consulta("key"), None),
    C::novo(
        "nominatim",
        "OpenStreetMap (Nominatim)",
        Mapas,
        "https://nominatim.openstreetmap.org",
        Nenhuma,
        Some("/search?q=Buriticupu&format=json&limit=1"),
    ),
    C::novo("restcountries", "REST Countries", Mapas, "https://restcountries.com/v3.1", Nenhuma, Some("/name/brazil")),
    // ------------------------------------------------------------------ ciência e livros
    C::novo(
        "wikipedia",
        "Wikipédia",
        Ciencia,
        "https://pt.wikipedia.org/api/rest_v1",
        Nenhuma,
        Some("/page/summary/Corvo"),
    ),
    C::novo(
        "wikidata",
        "Wikidata",
        Ciencia,
        "https://www.wikidata.org/w",
        Nenhuma,
        Some("/api.php?action=wbsearchentities&search=corvo&language=pt&format=json"),
    ),
    C::novo(
        "openlibrary",
        "Open Library",
        Ciencia,
        "https://openlibrary.org",
        Nenhuma,
        Some("/search.json?q=corvo&limit=1"),
    ),
    C::novo(
        "arxiv",
        "arXiv",
        Ciencia,
        "https://export.arxiv.org/api",
        Nenhuma,
        Some("/query?search_query=all:agents&max_results=1"),
    ),
    C::novo(
        "pubmed",
        "PubMed",
        Ciencia,
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils",
        Nenhuma,
        Some("/esearch.fcgi?db=pubmed&term=crow&retmode=json"),
    ),
    C::novo("crossref", "Crossref", Ciencia, "https://api.crossref.org", Nenhuma, Some("/works?query=crows&rows=1")),
    C::novo("nasa", "NASA", Ciencia, "https://api.nasa.gov", Consulta("api_key"), Some("/planetary/apod"))
        .dica("chave grátis em api.nasa.gov (DEMO_KEY serve para testar)"),
    C::novo("openfoodfacts", "Open Food Facts", Ciencia, "https://world.openfoodfacts.org/api/v2", Nenhuma, None)
        .dica("público: GET /product/CODIGO_DE_BARRAS"),
    // ------------------------------------------------------------------ Brasil
    C::novo("viacep", "ViaCEP", Brasil, "https://viacep.com.br/ws", Nenhuma, Some("/65393000/json/")),
    C::novo(
        "brasilapi",
        "BrasilAPI (CEP, CNPJ, bancos, feriados)",
        Brasil,
        "https://brasilapi.com.br/api",
        Nenhuma,
        Some("/cep/v2/65393000"),
    ),
    C::novo(
        "ibge",
        "IBGE",
        Brasil,
        "https://servicodados.ibge.gov.br/api",
        Nenhuma,
        Some("/v1/localidades/estados/MA/municipios"),
    ),
    C::novo(
        "bcb",
        "Banco Central (séries)",
        Brasil,
        "https://api.bcb.gov.br/dados/serie",
        Nenhuma,
        Some("/bcdata.sgs.432/dados/ultimos/1?formato=json"),
    ),
    C::novo(
        "camara",
        "Câmara dos Deputados",
        Brasil,
        "https://dadosabertos.camara.leg.br/api/v2",
        Nenhuma,
        Some("/deputados?siglaUf=MA&itens=1"),
    ),
    C::novo(
        "senado",
        "Senado Federal",
        Brasil,
        "https://legis.senado.leg.br/dadosabertos",
        Nenhuma,
        Some("/senador/lista/atual.json"),
    ),
    C::novo(
        "transparencia",
        "Portal da Transparência",
        Brasil,
        "https://api.portaldatransparencia.gov.br/api-de-dados",
        h("chave-api-dados"),
        None,
    )
    .dica("chave grátis no portal (cadastro com gov.br)"),
    C::novo("receitaws", "ReceitaWS (CNPJ)", Brasil, "https://receitaws.com.br/v1", Nenhuma, None)
        .dica("público: GET /cnpj/SO_NUMEROS"),
    // ------------------------------------------------------------------ automação e lazer
    C::novo("strava", "Strava", Automacao, "https://www.strava.com/api/v3", Bearer, Some("/athlete")),
    C::novo("lichess", "Lichess", Automacao, "https://lichess.org/api", Bearer, Some("/account")),
    C::novo("chesscom", "Chess.com", Automacao, "https://api.chess.com/pub", Nenhuma, None)
        .dica("público: GET /player/USUARIO"),
    C::novo("rawg", "RAWG (jogos)", Automacao, "https://api.rawg.io/api", Consulta("key"), Some("/games?page_size=1")),
    C::novo("steam", "Steam", Automacao, "https://api.steampowered.com", Consulta("key"), None),
    C::novo("riot", "Riot Games (Brasil)", Automacao, "https://br1.api.riotgames.com", h("X-Riot-Token"), None),
];
