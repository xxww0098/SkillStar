use crate::core::{
    marketplace,
    skill::{OfficialPublisher, Skill},
};

#[tauri::command]
pub async fn search_skills_sh(query: String) -> Result<marketplace::MarketplaceResult, String> {
    eprintln!("[search_skills_sh] Called with query: {}", query);
    match marketplace::search_skills_sh(&query, 200).await {
        Ok(result) => {
            eprintln!(
                "[search_skills_sh] Success, got {} skills",
                result.skills.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[search_skills_sh] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_skills_sh_leaderboard(category: String) -> Result<Vec<Skill>, String> {
    eprintln!(
        "[get_skills_sh_leaderboard] Called with category: {}",
        category
    );
    match marketplace::get_skills_sh_leaderboard(&category).await {
        Ok(result) => {
            eprintln!(
                "[get_skills_sh_leaderboard] Success, got {} skills",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[get_skills_sh_leaderboard] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_official_publishers() -> Result<Vec<OfficialPublisher>, String> {
    eprintln!("[get_official_publishers] Called");
    match marketplace::get_official_publishers().await {
        Ok(result) => {
            eprintln!(
                "[get_official_publishers] Success, got {} publishers",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[get_official_publishers] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn hydrate_marketplace_descriptions(
    requests: Vec<marketplace::MarketplaceDescriptionRequest>,
) -> Result<Vec<marketplace::MarketplaceDescriptionPatch>, String> {
    eprintln!(
        "[hydrate_marketplace_descriptions] Called with {} requests",
        requests.len()
    );
    match marketplace::hydrate_marketplace_descriptions(requests).await {
        Ok(result) => {
            eprintln!(
                "[hydrate_marketplace_descriptions] Success, got {} patches",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[hydrate_marketplace_descriptions] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repos(
    publisher_name: String,
) -> Result<Vec<marketplace::PublisherRepo>, String> {
    eprintln!(
        "[get_publisher_repos] Called for publisher: {}",
        publisher_name
    );
    match marketplace::get_publisher_repos(&publisher_name).await {
        Ok(repos) => {
            eprintln!(
                "[get_publisher_repos] Success, got {} repos",
                repos.len()
            );
            Ok(repos)
        }
        Err(e) => {
            eprintln!("[get_publisher_repos] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repo_skills(
    publisher_name: String,
    repo_name: String,
) -> Result<Vec<marketplace::PublisherRepoSkill>, String> {
    eprintln!(
        "[get_publisher_repo_skills] Called for {}/{}",
        publisher_name, repo_name
    );
    match marketplace::get_publisher_repo_skills(&publisher_name, &repo_name).await {
        Ok(skills) => {
            eprintln!(
                "[get_publisher_repo_skills] Success, got {} skills",
                skills.len()
            );
            Ok(skills)
        }
        Err(e) => {
            eprintln!("[get_publisher_repo_skills] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_marketplace_skill_details(
    source: String,
    name: String,
) -> Result<marketplace::MarketplaceSkillDetails, String> {
    eprintln!(
        "[get_marketplace_skill_details] Called for {}/{}",
        source, name
    );
    match marketplace::fetch_marketplace_skill_details(&source, &name).await {
        Ok(details) => {
            eprintln!(
                "[get_marketplace_skill_details] Success — summary: {}, readme: {}, audits: {}",
                details.summary.is_some(),
                details.readme.is_some(),
                details.security_audits.len()
            );
            Ok(details)
        }
        Err(e) => {
            eprintln!("[get_marketplace_skill_details] Error: {}", e);
            Err(e.to_string())
        }
    }
}
